use bytes::BytesMut;
use futures::future::BoxFuture;
use serde::{ser, Deserialize, Serialize};
use std::{
    cell::RefCell,
    error::Error,
    fmt,
    io::BufWriter,
    marker::PhantomData,
    rc::{Rc, Weak},
    sync::{Arc, Mutex},
};
use tokio::task;

use super::{
    io::{ChannelBytesWriter, LimitedBytesWriter},
    BIG_DATA_CHUNK_QUEUE, BIG_DATA_LIMIT,
};
use crate::{
    chmux,
    codec::{CodecT, SerializationError},
    rsync::handle::HandleStorage,
};

/// An error that occured during remote sending.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SendError<T> {
    /// Error kind.
    pub kind: SendErrorKind,
    /// Item that could not be sent.
    pub item: T,
}

/// Error kind that occured during remote sending.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SendErrorKind {
    /// Serialization of the item failed.
    Serialize(SerializationError),
    /// Sending of the serialized item over the chmux channel failed.
    Send(chmux::SendError),
}

impl<T> SendError<T> {
    pub(crate) fn new(kind: SendErrorKind, item: T) -> Self {
        Self { kind, item }
    }
}

impl fmt::Display for SendErrorKind {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Self::Serialize(err) => write!(f, "serialization error: {}", err),
            Self::Send(err) => write!(f, "send error: {}", err),
        }
    }
}

impl<T> fmt::Display for SendError<T> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", &self.kind)
    }
}

impl<T> Error for SendError<T> where T: fmt::Debug {}

/// Gathers ports to send to the remote endpoint during object serialization.
pub struct PortSerializer {
    allocator: chmux::PortAllocator,
    #[allow(clippy::type_complexity)]
    requests:
        Vec<(chmux::PortNumber, Box<dyn FnOnce(chmux::Connect) -> BoxFuture<'static, ()> + Send + 'static>)>,
    handle_storage: HandleStorage,
}

impl PortSerializer {
    thread_local! {
        static INSTANCE: RefCell<Weak<RefCell<PortSerializer>>> = RefCell::new(Weak::new());
    }

    /// Create a new port serializer and register it as active.
    fn start(allocator: chmux::PortAllocator, handle_storage: HandleStorage) -> Rc<RefCell<Self>> {
        let this = Rc::new(RefCell::new(Self { allocator, requests: Vec::new(), handle_storage }));
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
        callback: impl FnOnce(chmux::Connect) -> BoxFuture<'static, ()> + Send + 'static,
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

    /// Returns the handle storage of the channel multiplexer.
    pub(crate) fn handle_storage<E>() -> Result<HandleStorage, E>
    where
        E: serde::ser::Error,
    {
        let this = match Self::INSTANCE.with(|i| i.borrow().upgrade()) {
            Some(this) => this,
            None => return Err(ser::Error::custom("a handle can only be serialized for sending")),
        };
        let this = this.try_borrow().expect("PortSerializer is referenced multiple times during serialization");

        Ok(this.handle_storage.clone())
    }
}

/// Sends arbitrary values to a remote endpoint.
///
/// Values may be or contain any channel from this crate.
pub struct Sender<T, Codec> {
    sender: chmux::Sender,
    big_data: i8,
    _data: PhantomData<T>,
    _codec: PhantomData<Codec>,
}

impl<T, Codec> Sender<T, Codec>
where
    T: Serialize + Send + 'static,
    Codec: CodecT,
{
    /// Create a remote sender from a ChMux sender.
    pub fn new(sender: chmux::Sender) -> Self {
        Self { sender, big_data: 0, _data: PhantomData, _codec: PhantomData }
    }

    fn serialize_buffered(
        allocator: chmux::PortAllocator, handle_storage: HandleStorage, item: &T, limit: usize,
    ) -> Result<Option<(BytesMut, PortSerializer)>, SerializationError> {
        let mut lw = LimitedBytesWriter::new(limit);
        let ps_ref = PortSerializer::start(allocator, handle_storage);

        match <Codec as CodecT>::serialize(&mut lw, &item) {
            _ if lw.overflow() => return Ok(None),
            Ok(()) => (),
            Err(err) => return Err(err),
        };

        let ps = PortSerializer::finish(ps_ref);
        Ok(Some((lw.into_inner().unwrap(), ps)))
    }

    async fn serialize_streaming(
        allocator: chmux::PortAllocator, handle_storage: HandleStorage, item: T,
        tx: tokio::sync::mpsc::Sender<BytesMut>, chunk_size: usize,
    ) -> Result<(T, PortSerializer, usize), (SerializationError, T)> {
        let cbw = ChannelBytesWriter::new(tx);
        let mut cbw = BufWriter::with_capacity(chunk_size, cbw);

        let item_arc = Arc::new(Mutex::new(item));
        let item_arc_task = item_arc.clone();

        let result = task::spawn_blocking(move || {
            let ps_ref = PortSerializer::start(allocator, handle_storage);

            let item = item_arc_task.lock().unwrap();
            <Codec as CodecT>::serialize(&mut cbw, &*item)?;

            let cbw = cbw.into_inner().map_err(|_| {
                SerializationError::new(std::io::Error::new(std::io::ErrorKind::BrokenPipe, "flush failed"))
            })?;

            let ps = PortSerializer::finish(ps_ref);
            Ok((ps, cbw.written()))
        })
        .await;

        let item = match Arc::try_unwrap(item_arc) {
            Ok(item_mutex) => match item_mutex.into_inner() {
                Ok(item) => item,
                Err(err) => err.into_inner(),
            },
            Err(_) => unreachable!("serialization task has terminated"),
        };

        match result {
            Ok(Ok((ps, written))) => Ok((item, ps, written)),
            Ok(Err(err)) => Err((err, item)),
            Err(err) => Err((SerializationError::new(err), item)),
        }
    }

    /// Sends an item over the channel.
    ///
    /// The item may contain ports that will be serialized and connected as well.
    pub async fn send(&mut self, item: T) -> Result<(), SendError<T>> {
        // Determine if it is worthy to try buffered serialization.
        let data_ps = if self.big_data <= 0 {
            // Try buffered serialization.
            match Self::serialize_buffered(
                self.sender.port_allocator(),
                self.sender.handle_storage(),
                &item,
                self.sender.max_data_size(),
            ) {
                Ok(Some(v)) => {
                    self.big_data = (self.big_data - 1).max(-BIG_DATA_LIMIT);
                    Some(v)
                }
                Ok(None) => {
                    self.big_data = (self.big_data + 1).min(BIG_DATA_LIMIT);
                    None
                }
                Err(err) => return Err(SendError::new(SendErrorKind::Serialize(err), item)),
            }
        } else {
            // Buffered serialization unlikely to succeed.
            None
        };

        let (item, ps) = match data_ps {
            Some((data, ps)) => {
                // Send buffered data.
                if let Err(err) = self.sender.send(data.freeze()).await {
                    return Err(SendError::new(SendErrorKind::Send(err), item));
                }
                (item, ps)
            }

            None => {
                // Stream data while serializing.
                let (tx, mut rx) = tokio::sync::mpsc::channel(BIG_DATA_CHUNK_QUEUE);
                let ser_task = Self::serialize_streaming(
                    self.sender.port_allocator(),
                    self.sender.handle_storage(),
                    item,
                    tx,
                    self.sender.chunk_size(),
                );

                let mut sc = self.sender.send_chunks();
                let send_task = async {
                    while let Some(chunk) = rx.recv().await {
                        sc = sc.send(chunk.freeze()).await?;
                    }
                    Ok(sc)
                };

                match tokio::join!(ser_task, send_task) {
                    (Ok((item, ps, size)), Ok(sc)) => {
                        if let Err(err) = sc.finish().await {
                            return Err(SendError::new(SendErrorKind::Send(err), item));
                        }

                        if size <= self.sender.max_data_size() {
                            self.big_data = (self.big_data - 1).max(-BIG_DATA_LIMIT);
                        }

                        (item, ps)
                    }
                    (Ok((item, _, _)), Err(err)) => {
                        return Err(SendError::new(SendErrorKind::Send(err), item));
                    }
                    (Err((err, item)), _) => {
                        return Err(SendError::new(SendErrorKind::Serialize(err), item));
                    }
                }
            }
        };

        // Extract ports and connect callbacks.
        let mut ports = Vec::new();
        let mut callbacks = Vec::new();
        for (port, callback) in ps.requests {
            ports.push(port);
            callbacks.push(callback);
        }

        // Request connecting chmux ports.
        let connects = if ports.is_empty() {
            Vec::new()
        } else {
            match self.sender.connect(ports, true).await {
                Ok(connects) => connects,
                Err(err) => return Err(SendError::new(SendErrorKind::Send(err), item)),
            }
        };

        // Ensure that item is dropped before calling connection callbacks.
        drop(item);

        // Call callbacks of BaseSenders and BaseReceivers with obtained
        // chmux connect requests.
        //
        // We have to spawn a task for this to ensure cancellation safety.
        tokio::spawn(async move {
            for (callback, connect) in callbacks.into_iter().zip(connects.into_iter()) {
                callback(connect).await;
            }
        });

        Ok(())
    }
}
