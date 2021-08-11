use std::{
    cell::RefCell,
    collections::HashMap,
    marker::PhantomData,
    rc::{Rc, Weak},
};

use futures::{future::BoxFuture, Future, FutureExt};
use serde::{de::DeserializeOwned, ser, Deserialize, Serialize};

use crate::{Obtainer, codec::{CodecT, DeserializationError, SerializationError}};
use chmux::{Connect, ConnectError, DataBuf, ListenerError, PortAllocator, PortNumber, RawReceiver, RawSender, Received, Request};

/// Error obtaining a remote receiver.
#[derive(Clone)]
enum ObtainReceiverError {
    Dropped,
    Listener(ListenerError),
    Connect(ConnectError),
}

impl From<ListenerError> for ObtainReceiverError {
    fn from(err: ListenerError) -> Self {
        Self::Listener(err)
    }
}

impl From<ConnectError> for ObtainReceiverError {
    fn from(err: ConnectError) -> Self {
        Self::Connect(err)
    }
}

#[derive(Debug)]
pub enum ReceiveError {
    Receive(chmux::ReceiveError),
    Deserialize(DeserializationError),
    UnmatchedPort(u32),
    MissingPorts(Vec<u32>),
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
    allocator: PortAllocator,
    /// Callbacks by remote port.
    expected: HashMap<u32, Box<dyn FnOnce(Request) + Send + 'static>>,
}

impl PortDeserializer {
    thread_local! {
        static INSTANCE: RefCell<Weak<RefCell<PortDeserializer>>> = RefCell::new(Weak::new());
    }

    /// Create a new port deserializer and register it as active.
    fn start(allocator: PortAllocator) -> Rc<RefCell<PortDeserializer>> {
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
    /// Returns the port allocator and local port number.
    /// Calls the specified function with the connect request.
    pub fn accept<E>(
        remote_port: u32, callback: impl FnOnce(Request) + Send + 'static,
    ) -> Result<(PortAllocator, PortNumber), E>
    where
        E: serde::de::Error,
    {
        let this = match Self::INSTANCE.with(|i| i.borrow().upgrade()) {
            Some(this) => this,
            None => return Err(serde::de::Error::custom("a channel can only be deserialized during receiving")),
        };
        let this =
            this.try_borrow_mut().expect("PortDeserializer is referenced multiple times during serialization");
        this.expected.insert(remote_port, Box::new(callback));

        let local_port = this.allocator.try_allocate().ok_or(serde::de::Error::custom("ports exhausted"))?;
        Ok((this.allocator.clone(), local_port))
    }
}

/// Receives data from remote endpoint.
///
/// Can deserialize a base send and a base receiver.
pub(crate) struct RemoteReceiver<T, Codec> {
    receiver: Obtainer<chmux::RawReceiver, ObtainReceiverError>,
    allocator: chmux::PortAllocator,
    data: Option<DataBuf>,
    item: Option<T>,
    gatherer: Option<PortDeserializer>,
    _codec: PhantomData<Codec>,
}

impl<T, Codec> RemoteReceiver<T, Codec>
where
    T: DeserializeOwned + Send + 'static,
    Codec: CodecT,
{

    pub fn new(receiver: Obtainer<RawReceiver, ObtainReceiverError>, allocator: PortAllocator) -> Self {
        Self { receiver, allocator, data: None, item: None, gatherer: None, _codec: PhantomData }
    }

    pub async fn recv(&mut self) -> Result<Option<T>, ReceiveError> {
        let receiver = self.receiver.get().await?;

        loop {
            if self.item.is_none() || self.gatherer.is_none() {
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

            // Connnect ports, if there are any.
            if !self.gatherer.as_ref().unwrap().expected.is_empty() {
                // Receive port requests from chmux.
                // TODO: reassemble ports if more ports than allowed per message
                let requests = match receiver.recv_any().await? {
                    Some(Received::Requests(requests)) => requests,
                    Some(Received::Data(data)) => {
                        // Current send operation has been aborted and this is data from
                        // next send operation, so we restart.
                        self.data = Some(data);
                        self.item = None;
                        self.gatherer = None;
                        continue;
                    }
                    None => {
                        self.data = None;
                        self.item = None;
                        self.gatherer = None;
                        return Ok(None);
                    }
                };

                // Call port callbacks from received objects.
                let mut expected = self.gatherer.take().unwrap().expected;
                for request in requests {
                    match expected.remove(&request.remote_port()) {
                        Some(callback) => callback(request),
                        None => return Err(ReceiveError::UnmatchedPort(request.remote_port())),
                    }
                }

                if !expected.is_empty() {
                    return Err(ReceiveError::MissingPorts(expected.into_iter().map(|(port, _)| port).collect()));
                }
            }

            return Ok(Some(self.item.take().unwrap()));
        }
    }

    pub async fn close(&mut self) {
        if let Ok(receiver) = self.receiver.get().await {
            receiver.close().await
        }
    }
}
