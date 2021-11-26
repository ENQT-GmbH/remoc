use bytes::{Buf, Bytes};
use futures::future::BoxFuture;
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use std::{
    cell::RefCell,
    collections::HashMap,
    error::Error,
    fmt,
    marker::PhantomData,
    panic,
    rc::{Rc, Weak},
};
use tokio::task::{self, JoinHandle};

use super::{io::ChannelBytesReader, BIG_DATA_CHUNK_QUEUE};
use crate::{
    chmux::{self, AnyStorage, Received, RecvChunkError},
    codec::{self, DeserializationError},
};

/// An error that occurred during receiving from a remote endpoint.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum RecvError {
    /// Receiving data over the chmux channel failed.
    Receive(chmux::RecvError),
    /// Deserialization of received data failed.
    Deserialize(DeserializationError),
    /// Chmux ports required for deserialization of received channels were not received.
    MissingPorts(Vec<u32>),
}

impl From<chmux::RecvError> for RecvError {
    fn from(err: chmux::RecvError) -> Self {
        Self::Receive(err)
    }
}

impl From<DeserializationError> for RecvError {
    fn from(err: DeserializationError) -> Self {
        Self::Deserialize(err)
    }
}

impl fmt::Display for RecvError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Self::Receive(err) => write!(f, "receive error: {}", err),
            Self::Deserialize(err) => write!(f, "deserialization error: {}", err),
            Self::MissingPorts(ports) => write!(
                f,
                "missing chmux ports: {}",
                ports.iter().map(|p| p.to_string()).collect::<Vec<_>>().join(", ")
            ),
        }
    }
}

impl Error for RecvError {}

impl RecvError {
    /// Returns whether the error is final, i.e. no further receive operation can succeed.
    pub fn is_final(&self) -> bool {
        match self {
            Self::Receive(err) => err.is_final(),
            Self::Deserialize(_) | Self::MissingPorts(_) => false,
        }
    }
}

/// Gathers ports sent from the remote endpoint during deserialization.
pub struct PortDeserializer {
    allocator: chmux::PortAllocator,
    /// Callbacks by remote port.
    #[allow(clippy::type_complexity)]
    expected: HashMap<
        u32,
        (
            chmux::PortNumber,
            Box<dyn FnOnce(chmux::PortNumber, chmux::Request) -> BoxFuture<'static, ()> + Send + 'static>,
        ),
    >,
    storage: AnyStorage,
}

impl PortDeserializer {
    thread_local! {
        static INSTANCE: RefCell<Weak<RefCell<PortDeserializer>>> = RefCell::new(Weak::new());
    }

    /// Create a new port deserializer and register it as active.
    fn start(allocator: chmux::PortAllocator, storage: AnyStorage) -> Rc<RefCell<PortDeserializer>> {
        let this = Rc::new(RefCell::new(Self { allocator, expected: HashMap::new(), storage }));
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
    #[inline]
    pub fn accept<E>(
        remote_port: u32,
        callback: impl FnOnce(chmux::PortNumber, chmux::Request) -> BoxFuture<'static, ()> + Send + 'static,
    ) -> Result<u32, E>
    where
        E: serde::de::Error,
    {
        let this = match Self::INSTANCE.with(|i| i.borrow().upgrade()) {
            Some(this) => this,
            None => return Err(serde::de::Error::custom("a channel can only be deserialized during receiving")),
        };

        let mut this =
            this.try_borrow_mut().expect("PortDeserializer is referenced multiple times during deserialization");
        let local_port =
            this.allocator.try_allocate().ok_or_else(|| serde::de::Error::custom("ports exhausted"))?;
        let local_port_num = *local_port;
        this.expected.insert(remote_port, (local_port, Box::new(callback)));

        Ok(local_port_num)
    }

    /// Returns the data storage of the channel multiplexer.
    #[inline]
    pub fn storage<E>() -> Result<AnyStorage, E>
    where
        E: serde::de::Error,
    {
        let this = match Self::INSTANCE.with(|i| i.borrow().upgrade()) {
            Some(this) => this,
            None => return Err(serde::de::Error::custom("a handle can only be deserialized during receiving")),
        };
        let this =
            this.try_borrow().expect("PortDeserializer is referenced multiple times during deserialization");

        Ok(this.storage.clone())
    }
}

/// Receives arbitrary values from a remote endpoint.
///
/// Values may be or contain any channel from this crate.
pub struct Receiver<T, Codec = codec::Default> {
    receiver: chmux::Receiver,
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
    Codec: codec::Codec,
{
    /// Creates a base remote receiver from a [chmux] receiver.
    pub fn new(receiver: chmux::Receiver) -> Self {
        Self {
            receiver,
            recved: None,
            data: DataSource::None,
            item: None,
            port_deser: None,
            default_max_ports: None,
            _codec: PhantomData,
        }
    }

    /// Receive an item from the remote endpoint.
    #[inline]
    pub async fn recv(&mut self) -> Result<Option<T>, RecvError> {
        if self.default_max_ports.is_none() {
            self.default_max_ports = Some(self.receiver.max_ports());
        }

        'restart: loop {
            if self.item.is_none() {
                // Receive data or start streaming it.
                if let DataSource::None = &self.data {
                    if self.recved.is_none() {
                        self.recved = Some(self.receiver.recv_any().await?);
                    }

                    self.data = match self.recved.take().unwrap() {
                        Some(Received::Data(data)) => DataSource::Buffered(Some(data)),
                        Some(Received::Chunks) => {
                            // Start deserialization thread.
                            let allocator = self.receiver.port_allocator();
                            let handle_storage = self.receiver.storage();
                            let (tx, rx) = tokio::sync::mpsc::channel(BIG_DATA_CHUNK_QUEUE);
                            let task = task::spawn_blocking(move || {
                                let cbr = ChannelBytesReader::new(rx);

                                let pds_ref = PortDeserializer::start(allocator, handle_storage);
                                let item = <Codec as codec::Codec>::deserialize(cbr)?;
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

                        let pdf_ref =
                            PortDeserializer::start(self.receiver.port_allocator(), self.receiver.storage());
                        self.item = Some(<Codec as codec::Codec>::deserialize(data.reader())?);
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

                                match self.receiver.recv_chunk().await {
                                    Ok(Some(chunk)) => tx_permit.send(Ok(chunk)),
                                    Ok(None) => break Ok(()),
                                    Err(err) => break Err(err),
                                }
                            };

                            match res {
                                Ok(()) => (),
                                Err(RecvChunkError::Cancelled) => {
                                    self.data = DataSource::None;
                                    continue 'restart;
                                }
                                Err(RecvChunkError::ChMux) => {
                                    self.data = DataSource::None;
                                    return Err(RecvError::Receive(chmux::RecvError::ChMux));
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
                                return Err(RecvError::Deserialize(err));
                            }
                            Err(err) => {
                                self.data = DataSource::None;
                                match err.try_into_panic() {
                                    Ok(payload) => panic::resume_unwind(payload),
                                    Err(err) => {
                                        return Err(RecvError::Deserialize(DeserializationError::new(err)))
                                    }
                                }
                            }
                        }
                    }
                }
            }

            // Connect received ports.
            let pds = self.port_deser.as_mut().unwrap();
            if !pds.expected.is_empty() {
                // Set port limit.
                //
                // Allow the reception of additional ports for forward compatibility,
                // i.e. our deserializer may use an older version of the struct
                // which is missing some ports that the remote endpoint sent.
                self.receiver.set_max_ports(pds.expected.len() + self.default_max_ports.unwrap());

                // Receive port requests from chmux.
                let requests = match self.receiver.recv_any().await? {
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

                // Call port callbacks from received objects, ignoring superfluous requests for
                // forward compatibility.
                for request in requests {
                    if let Some((local_port, callback)) = pds.expected.remove(&request.remote_port()) {
                        tokio::spawn(callback(local_port, request));
                    }
                }

                // But error on ports that we expect but that are missing.
                if !pds.expected.is_empty() {
                    return Err(RecvError::MissingPorts(pds.expected.iter().map(|(port, _)| *port).collect()));
                }
            }

            return Ok(Some(self.item.take().unwrap()));
        }
    }

    /// Close the channel.
    ///
    /// This stops the remote endpoint from sending more data, but allows already sent data
    /// to be received.
    #[inline]
    pub async fn close(&mut self) {
        self.receiver.close().await
    }
}
