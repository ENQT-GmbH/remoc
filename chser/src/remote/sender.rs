use std::{
    cell::RefCell,
    marker::PhantomData,
    rc::{Rc, Weak},
};

use futures::future::BoxFuture;
use serde::{ser, Serialize};

use super::Obtainer;
use crate::codec::{CodecT, SerializationError};

/// Error obtaining a remote sender.
#[derive(Clone, Debug)]
pub enum ObtainSenderError {
    Dropped,
    Listener(chmux::ListenerError),
    Connect(chmux::ConnectError),
}

impl From<chmux::ListenerError> for ObtainSenderError {
    fn from(err: chmux::ListenerError) -> Self {
        Self::Listener(err)
    }
}

impl From<chmux::ConnectError> for ObtainSenderError {
    fn from(err: chmux::ConnectError) -> Self {
        Self::Connect(err)
    }
}

pub struct SendError<T> {
    pub kind: SendErrorKind,
    pub item: T,
}

#[derive(Debug, Clone)]
pub enum SendErrorKind {
    Closed,
    PortsExhausted,
    Serialize(SerializationError),
    Send(chmux::SendError),
    Obtain(ObtainSenderError),
    Connect(chmux::ConnectError),
    Forward,
}

impl<T> SendError<T> {
    pub fn new(kind: SendErrorKind, item: T) -> Self {
        Self { kind, item }
    }
}

/// Gathers ports to send to the remote endpoint during object serialization.
pub(crate) struct PortSerializer {
    allocator: chmux::PortAllocator,
    #[allow(clippy::type_complexity)]
    requests: Vec<(
        chmux::PortNumber,
        Box<dyn FnOnce(chmux::Connect, chmux::PortAllocator) -> BoxFuture<'static, ()> + Send + 'static>,
    )>,
}

impl PortSerializer {
    thread_local! {
        static INSTANCE: RefCell<Weak<RefCell<PortSerializer>>> = RefCell::new(Weak::new());
    }

    /// Create a new port serializer and register it as active.
    fn start(allocator: chmux::PortAllocator) -> Rc<RefCell<Self>> {
        let this = Rc::new(RefCell::new(Self { allocator, requests: Vec::new() }));
        let weak = Rc::downgrade(&this);
        Self::INSTANCE.with(move |i| i.replace(weak));
        this
    }

    /// Deregister the active port serializer and return it.
    fn finish(this: Rc<RefCell<Self>>) -> Self {
        match Rc::try_unwrap(this) {
            Ok(i) => i.into_inner(),
            Err(_) => panic!("PortSerializer is referenced after serialization finished"),
        }
    }

    /// Open a chmux port to the remote endpoint.
    ///
    /// Returns the local port number and calls the specified function with the connect object.
    pub(crate) fn connect<E>(
        callback: impl FnOnce(chmux::Connect, chmux::PortAllocator) -> BoxFuture<'static, ()> + Send + 'static,
    ) -> Result<u32, E>
    where
        E: serde::ser::Error,
    {
        let this = match Self::INSTANCE.with(|i| i.borrow().upgrade()) {
            Some(this) => this,
            None => return Err(ser::Error::custom("a channel can only be serialized for sending")),
        };
        let mut this =
            this.try_borrow_mut().expect("PortSerializer is referenced multiple times during serialization");

        let local_port = this.allocator.try_allocate().ok_or_else(|| ser::Error::custom("ports exhausted"))?;
        let local_port_num = *local_port;
        this.requests.push((local_port, Box::new(callback)));

        Ok(local_port_num)
    }
}

/// Sends data to a remote endpoint.
///
/// Can serialize a base sender and a base receiver.
pub(crate) struct Sender<T, Codec> {
    sender: Obtainer<chmux::Sender, ObtainSenderError>,
    allocator: chmux::PortAllocator,
    _data: PhantomData<T>,
    _codec: PhantomData<Codec>,
}

impl<T, Codec> Sender<T, Codec>
where
    T: Serialize + Send + 'static,
    Codec: CodecT,
{
    pub fn new(sender: Obtainer<chmux::Sender, ObtainSenderError>, allocator: chmux::PortAllocator) -> Self {
        Self { sender, allocator, _data: PhantomData, _codec: PhantomData }
    }

    pub async fn send(&mut self, item: T) -> Result<(), SendError<T>> {
        // Serialize item while gathering port requests from embedded
        // BaseSenders and BaseReceivers.
        let data;
        let requestor = {
            let requestor_ref = PortSerializer::start(self.allocator.clone());
            data = match Codec::serialize(&item) {
                Ok(data) => data,
                Err(err) => return Err(SendError::new(SendErrorKind::Serialize(err), item)),
            };
            PortSerializer::finish(requestor_ref)
        };

        // Send the serialized item data.
        let sender = match self.sender.get().await {
            Ok(sender) => sender,
            Err(err) => return Err(SendError::new(SendErrorKind::Obtain(err), item)),
        };
        if let Err(err) = sender.send(data).await {
            return Err(SendError::new(SendErrorKind::Send(err), item));
        }

        // Extract ports and connect callbacks.
        let mut ports = Vec::new();
        let mut callbacks = Vec::new();
        for (port, callback) in requestor.requests {
            ports.push(port);
            callbacks.push(callback);
        }

        // Request connecting chmux ports.
        // TODO: split over multiple messages if too many ports.
        let connects = match sender.connect(ports, true).await {
            Ok(connects) => connects,
            Err(err) => return Err(SendError::new(SendErrorKind::Send(err), item)),
        };

        // Ensure that item is dropped before calling connection callbacks.
        drop(item);

        // Call callbacks of BaseSenders and BaseReceivers with obtained
        // chmux connect requests.
        //
        // We have to spawn a task for this to ensure cancellation safety.
        let allocator = self.allocator.clone();
        tokio::spawn(async move {
            for (callback, connect) in callbacks.into_iter().zip(connects.into_iter()) {
                callback(connect, allocator.clone()).await;
            }
        });

        Ok(())
    }
}
