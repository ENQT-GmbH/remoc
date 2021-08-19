use std::{
    cell::RefCell,
    collections::HashMap,
    marker::PhantomData,
    rc::{Rc, Weak},
};

use chmux::Request;
use futures::future::BoxFuture;
use serde::{de::DeserializeOwned, Deserialize, Serialize};

use super::Obtainer;
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
    UnmatchedPort(u32),
    MissingPorts(Vec<u32>),
    Connect(chmux::ConnectError),
    Listen(chmux::ListenerError),
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
    data: Option<chmux::DataBuf>,
    item: Option<T>,
    gatherer: Option<PortDeserializer>,
    _codec: PhantomData<Codec>,
}

impl<T, Codec> Receiver<T, Codec>
where
    T: DeserializeOwned + Send + 'static,
    Codec: CodecT,
{
    pub fn new(
        receiver: Obtainer<chmux::Receiver, ObtainReceiverError>, allocator: chmux::PortAllocator,
    ) -> Self {
        Self { receiver, allocator, data: None, item: None, gatherer: None, _codec: PhantomData }
    }

    pub async fn recv(&mut self) -> Result<Option<T>, ReceiveError> {
        let receiver = self.receiver.get().await?;

        'restart: loop {
            if self.item.is_none() {
                // Receive data.
                if self.data.is_none() {
                    // This will automatically drop all port messages up to the
                    // next data message.
                    self.data = match receiver.recv().await? {
                        Some(data) => Some(data),
                        None => return Ok(None),
                    };
                }

                // Deserialize it while keeping track of requested ports.
                let gatherer = PortDeserializer::start(self.allocator.clone());
                self.item = Some(Codec::deserialize(self.data.take().unwrap())?);
                self.gatherer = Some(PortDeserializer::finish(gatherer));
            }

            // Connnect received ports.
            let gatherer = self.gatherer.as_mut().unwrap();
            while !gatherer.expected.is_empty() {
                // Receive port requests from chmux.
                let requests = match receiver.recv_any().await? {
                    Some(chmux::Received::Requests(requests)) => requests,
                    Some(chmux::Received::Data(data)) => {
                        // Current send operation has been aborted and this is data from
                        // next send operation, so we restart.
                        self.data = Some(data);
                        self.item = None;
                        continue 'restart;
                    }
                    None => {
                        self.data = None;
                        self.item = None;
                        self.gatherer = None;
                        return Ok(None);
                    }
                };

                // Call port callbacks from received objects.
                for request in requests {
                    match gatherer.expected.remove(&request.remote_port()) {
                        Some((local_port, callback)) => {
                            tokio::spawn(callback(local_port, request, self.allocator.clone()));
                        }
                        None => return Err(ReceiveError::UnmatchedPort(request.remote_port())),
                    }
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
