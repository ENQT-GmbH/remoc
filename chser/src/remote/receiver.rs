use std::{
    cell::RefCell,
    collections::HashMap,
    marker::PhantomData,
    rc::{Rc, Weak},
};

use bytes::{Buf, Bytes};
use chmux::{ReceiveChunkError, Received};
use futures::future::BoxFuture;
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use tokio::task::{self, JoinHandle};

use super::{io::ChannelBytesReader, Obtainer, BIG_DATA_CHUNK_QUEUE};
use crate::codec::{CodecT, DeserializationError};

/// Error obtaining a remote receiver.
#[derive(Clone)]
pub enum ObtainReceiverError {
    Dropped,
    Listener(chmux::ListenerError),
    Connect(chmux::ConnectError),
}

impl From<chmux::ListenerError> for ObtainReceiverError {
    fn from(err: chmux::ListenerError) -> Self {
        Self::Listener(err)
    }
}

impl From<chmux::ConnectError> for ObtainReceiverError {
    fn from(err: chmux::ConnectError) -> Self {
        Self::Connect(err)
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub enum ReceiveError {
    Receive(chmux::ReceiveError),
    Deserialize(DeserializationError),
    MissingPorts(Vec<u32>),
    Connect(chmux::ConnectError),
    Listen(chmux::ListenerError),
    Multiplexer,
}

impl From<chmux::ReceiveError> for ReceiveError {
    fn from(err: chmux::ReceiveError) -> Self {
        Self::Receive(err)
    }
}

impl From<DeserializationError> for ReceiveError {
    fn from(err: DeserializationError) -> Self {
        Self::Deserialize(err)
    }
}

impl From<ObtainReceiverError> for ReceiveError {
    fn from(_: ObtainReceiverError) -> Self {
        todo!()
    }
}

/// Gathers ports sent from the remote endpoint during deserialization.
pub(crate) struct PortDeserializer {
    allocator: chmux::PortAllocator,
    /// Callbacks by remote port.
    #[allow(clippy::type_complexity)]
    expected: HashMap<
        u32,
        (
            chmux::PortNumber,
            Box<
                dyn FnOnce(chmux::PortNumber, chmux::Request, chmux::PortAllocator) -> BoxFuture<'static, ()>
                    + Send
                    + 'static,
            >,
        ),
    >,
}

impl PortDeserializer {
    thread_local! {
        static INSTANCE: RefCell<Weak<RefCell<PortDeserializer>>> = RefCell::new(Weak::new());
    }

    /// Create a new port deserializer and register it as active.
    fn start(allocator: chmux::PortAllocator) -> Rc<RefCell<PortDeserializer>> {
        let this = Rc::new(RefCell::new(Self { allocator, expected: HashMap::new() }));
        let weak = Rc::downgrade(&this);
        Self::INSTANCE.with(move |i| i.replace(weak));
        this
    }

    /// Deregister the active port deserializer and return it.
    fn finish(this: Rc<RefCell<PortDeserializer>>) -> Self {
        match Rc::try_unwrap(this) {
            Ok(i) => i.into_inner(),
            Err(_) => panic!("PortDeserializer is referenced after deserialization finished"),
        }
    }

    /// Accept the chmux port with the specified remote port number sent from the remote endpoint.
    ///
    /// Returns the local port number and calls the specified function with the received connect request.
    pub fn accept<E>(
        remote_port: u32,
        callback: impl FnOnce(chmux::PortNumber, chmux::Request, chmux::PortAllocator) -> BoxFuture<'static, ()>
            + Send
            + 'static,
    ) -> Result<u32, E>
    where
        E: serde::de::Error,
    {
        let this = match Self::INSTANCE.with(|i| i.borrow().upgrade()) {
            Some(this) => this,
            None => return Err(serde::de::Error::custom("a channel can only be deserialized during receiving")),
        };

        let mut this =
            this.try_borrow_mut().expect("PortDeserializer is referenced multiple times during serialization");
        let local_port =
            this.allocator.try_allocate().ok_or_else(|| serde::de::Error::custom("ports exhausted"))?;
        let local_port_num = *local_port;
        this.expected.insert(remote_port, (local_port, Box::new(callback)));

        Ok(local_port_num)
    }
}

/// Receives data from remote endpoint.
///
/// Can deserialize a base send and a base receiver.
pub(crate) struct Receiver<T, Codec> {
    receiver: Obtainer<chmux::Receiver, ObtainReceiverError>,
    allocator: chmux::PortAllocator,
    recved: Option<Option<Received>>,
    data: DataSource<T>,
    item: Option<T>,
    port_deser: Option<PortDeserializer>,
    default_max_ports: Option<usize>,
    _codec: PhantomData<Codec>,
}

enum DataSource<T> {
    None,
    Buffered(Option<chmux::DataBuf>),
    Streamed {
        tx: Option<tokio::sync::mpsc::Sender<Result<Bytes, ()>>>,
        task: JoinHandle<Result<(T, PortDeserializer), DeserializationError>>,
    },
}

impl<T, Codec> Receiver<T, Codec>
where
    T: DeserializeOwned + Send + 'static,
    Codec: CodecT,
{
    pub fn new(
        receiver: Obtainer<chmux::Receiver, ObtainReceiverError>, allocator: chmux::PortAllocator,
    ) -> Self {
        Self {
            receiver,
            allocator,
            recved: None,
            data: DataSource::None,
            item: None,
            port_deser: None,
            default_max_ports: None,
            _codec: PhantomData,
        }
    }

    pub async fn recv(&mut self) -> Result<Option<T>, ReceiveError> {
        let receiver = self.receiver.get().await?;

        if self.default_max_ports.is_none() {
            self.default_max_ports = Some(receiver.max_ports());
        }

        'restart: loop {
            if self.item.is_none() {
                // Receive data or start streaming it.
                if let DataSource::None = &self.data {
                    if self.recved.is_none() {
                        self.recved = Some(receiver.recv_any().await?);
                    }

                    self.data = match self.recved.take().unwrap() {
                        Some(Received::Data(data)) => DataSource::Buffered(Some(data)),
                        Some(Received::BigData) => {
                            // Start deserialization thread.
                            let allocator = self.allocator.clone();
                            let (tx, rx) = tokio::sync::mpsc::channel(BIG_DATA_CHUNK_QUEUE);
                            let task = task::spawn_blocking(move || {
                                let cbr = ChannelBytesReader::new(rx);

                                let pds_ref = PortDeserializer::start(allocator);
                                let item = Codec::deserialize(cbr)?;
                                let pds = PortDeserializer::finish(pds_ref);

                                Ok((item, pds))
                            });
                            DataSource::Streamed { tx: Some(tx), task }
                        }
                        Some(Received::Requests(_)) => continue 'restart,
                        None => return Ok(None),
                    };
                }

                // Deserialize data.
                match &mut self.data {
                    DataSource::None => unreachable!(),

                    // Deserialize data from buffer.
                    DataSource::Buffered(data_opt) => {
                        let data = if let Some(data) = data_opt {
                            data
                        } else {
                            self.data = DataSource::None;
                            continue 'restart;
                        };

                        let pdf_ref = PortDeserializer::start(self.allocator.clone());
                        self.item = Some(Codec::deserialize(data.reader())?);
                        self.port_deser = Some(PortDeserializer::finish(pdf_ref));

                        self.data = DataSource::None;
                    }

                    // Observe deserialization of streamed data.
                    DataSource::Streamed { tx, task } => {
                        // Feed received data chunks to deserialization thread.
                        if let Some(tx) = &tx {
                            let res = loop {
                                let tx_permit = if let Ok(tx_permit) = tx.reserve().await {
                                    tx_permit
                                } else {
                                    break Ok(());
                                };

                                match receiver.recv_chunk().await {
                                    Ok(Some(chunk)) => tx_permit.send(Ok(chunk)),
                                    Ok(None) => break Ok(()),
                                    Err(err) => break Err(err),
                                }
                            };

                            match res {
                                Ok(()) => (),
                                Err(ReceiveChunkError::Cancelled) => {
                                    self.data = DataSource::None;
                                    continue 'restart;
                                }
                                Err(ReceiveChunkError::Multiplexer) => {
                                    self.data = DataSource::None;
                                    return Err(ReceiveError::Multiplexer);
                                }
                            }
                        }
                        *tx = None;

                        // Get deserialized item.
                        match task.await {
                            Ok(Ok((item, pds))) => {
                                self.item = Some(item);
                                self.port_deser = Some(pds);
                                self.data = DataSource::None;
                            }
                            Ok(Err(err)) => {
                                self.data = DataSource::None;
                                return Err(ReceiveError::Deserialize(err));
                            }
                            Err(err) => {
                                self.data = DataSource::None;
                                return Err(ReceiveError::Deserialize(DeserializationError::new(err)));
                            }
                        }
                    }
                }
            }

            // Connnect received ports.
            let pds = self.port_deser.as_mut().unwrap();
            if !pds.expected.is_empty() {
                // Set port limit.
                //
                // Allow the reception of additional ports for forward compatibility,
                // i.e. our deserializer may use an older version of the struct
                // which is missing some ports that the remote endpoint sent.
                receiver.set_max_ports(pds.expected.len() + self.default_max_ports.unwrap());

                // Receive port requests from chmux.
                let requests = match receiver.recv_any().await? {
                    Some(chmux::Received::Requests(requests)) => requests,
                    other => {
                        // Current send operation has been aborted and this is data from
                        // next send operation, so we restart.
                        self.recved = Some(other);
                        self.data = DataSource::None;
                        self.item = None;
                        self.port_deser = None;
                        continue 'restart;
                    }
                };

                // Call port callbacks from received objects, ignoring superflous requests for
                // forward compatibility.
                for request in requests {
                    if let Some((local_port, callback)) = pds.expected.remove(&request.remote_port()) {
                        tokio::spawn(callback(local_port, request, self.allocator.clone()));
                    }
                }

                // But error on ports that we expect but that are missing.
                if !pds.expected.is_empty() {
                    return Err(ReceiveError::MissingPorts(pds.expected.iter().map(|(port, _)| *port).collect()));
                }
            }

            return Ok(Some(self.item.take().unwrap()));
        }
    }

    #[allow(dead_code)]
    pub async fn close(&mut self) {
        if let Ok(receiver) = self.receiver.get().await {
            receiver.close().await
        }
    }
}
