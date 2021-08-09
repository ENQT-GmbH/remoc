use std::cell::{Cell, RefCell};
use std::error::Error;
use std::marker::PhantomData;
use std::mem;
use std::rc::{Rc, Weak};
use std::sync::Arc;

use chmux::{PortNumber, RawReceiver};
use futures::FutureExt;
use futures::future::BoxFuture;
use serde::de::DeserializeOwned;
use serde::{ser, Serialize};
use tokio::sync::Mutex;

pub mod mpsc;

pub struct Distributor {
    client: chmux::RawClient,
    listener: chmux::RawListener,
}

enum DistributorReq {
    SendBasePort(),
}

impl Distributor {
    pub fn new<S, R>(
        client: chmux::RawClient, listener: chmux::RawListener,
    ) -> (Self, mpsc::Sender<S>, mpsc::Receiver<R>) {
        // run our own mux or use running mux?
        // take sink and stream
        // and mux run method over distributor method?
        // maybe? why not?
        // also we should think about request/response channel
        // the way we always do it here.
        // so if we take a client and server we have nothing to fear about lifetimes?
        // yes, but we might also perform a clean exit if possible?
        // so benefit of running muxer in user function is that user can
        // obtain error.
        let this = Self { client, listener };

        // the sender and receiver must be connected.
        todo!()
    }

    pub async fn run(self) -> Result<(), ()> {
        // how will we connect to a channel?
        // it will be send over us
        todo!()
    }
}

thread_local! {
    static MANAGER: RefCell<Weak<RefCell<ChManager>>> = RefCell::new(Weak::new());
}

enum ChManagerError {
    PortsExhausted,
    Terminated,
}

impl<T> From<tokio::sync::mpsc::error::SendError<T>> for ChManagerError {
    fn from(_: tokio::sync::mpsc::error::SendError<T>) -> Self {
        Self::Terminated
    }
}

struct ChManager {
    port_allocator: chmux::PortAllocator,
    sender_tx: tokio::sync::mpsc::UnboundedSender<(PortNumber, Box<dyn FnOnce(RawReceiver) -> BoxFuture<'static, ()>>)>
}

impl ChManager {
    pub(crate) fn prepare_send_of_sender(
        &self, after_send_fn: impl FnOnce(RawReceiver) -> BoxFuture<'static, ()> + 'static,
    ) -> Result<u32, ChManagerError> {
        let local_port = self.port_allocator.try_allocate().ok_or(ChManagerError::PortsExhausted)?;
        let local_port_num = *local_port;
        self.sender_tx.send((local_port, Box::new(after_send_fn)))?;
        Ok(local_port_num)
    }
}

pub enum SendError {
    Closed,
    Serialize(Box<dyn Error>),
}

#[derive(Debug)]
pub enum BaseReceiveError {
    ReceiveError(chmux::ReceiveError),
}

impl From<chmux::ReceiveError> for BaseReceiveError {
    fn from(err: chmux::ReceiveError) -> Self {
        Self::ReceiveError(err)
    }
}

#[derive(Debug)]
pub enum BaseSendError {
    SendError(chmux::SendError),
}

impl From<chmux::SendError> for BaseSendError {
    fn from(err: chmux::SendError) -> Self {
        Self::SendError(err)
    }
}

pub struct BaseSender<T> {
    receiver_source_tx: tokio::sync::mpsc::UnboundedSender<ReceiverSource<T>>,
}

impl<T> BaseSender<T>
where
    T: Serialize,
{
    pub async fn send(&self, item: &T) -> Result<(), SendError> {
        todo!()
    }
}

impl<T> Serialize for BaseSender<T> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let manager = match MANAGER.with(|m| m.borrow().upgrade()) {
            Some(manager) => manager,
            None => return Err(ser::Error::custom("channel manager missing")),
        };

        let manager = manager.borrow_mut();
        // So what does it need to do here?

        // so during serialization we need to do the following:
        // 1. create a ReceiverSource::Remote with a RemoteReceiver (how to wire up?)
        // after successfully sending serialized data over chmux channel
        // 2. send the new receiver source over the receiver_source_tx channel
        // 3. close the current sender
        // note that sending over chmux can fail, so we need a commit/rollback stage for this case
        //
        // so how will wire up work?
        // 1. we replace ourselves by a send channel identifier
        // 1b. this send channel identifier must be registered with the channel manager
        // 1c. it cannot directly open a channel during registration because we run
        // from a non-async function
        // 2. the obtained id is serialized as our value
        //
        // there must be one manager per chmux because the identifier matching must be unique
        // would there be an alternative way to obtain a port number sync?
        // Yes, client could be extended.
        //
        // So we wish to do the following:
        // 1. pre-allocate a port
        // 2. connect with that port
        // 3. on server side inspect requests => shall this return credits?
        //    => yes, because server software can manage port allocation table themselves
        // => done!
        //
        // next step?
        // we must now handle the port allocation
        // for now perform try allocation and if it fails, return error

        let port = manager.prepare_send_of_sender(|recv| async move {
            self.receiver_source_tx.send(ReceiverSource::Remote(RemoteReceiver::new(recv)));
        }.boxed());

        // todo: encode the returned port number as serialization data

    }
}

// Is a channel that exists and sends items over remote conneciton.
struct RemoteSender<T> {
    sender: chmux::RawSender,
    _phantom: PhantomData<T>,
}

impl<T> RemoteSender<T>
where
    T: Serialize,
{
    pub async fn send(&self, item: T) -> Result<(), BaseSendError> {
        // So assuming we send a sender.

        let manager = Rc::new(RefCell::new(ChManager {}));
        let manager_weak = Rc::downgrade(&manager);
        MANAGER.with(|m| m.replace(manager_weak));
        //MANAGER.replace(manager);

        let data = serde_json::to_vec(item).map_err(|err| SendError::Serialize(Box::new(err)))?;

        let manager = match Rc::try_unwrap(manager) {
            Ok(manager) => manager,
            Err(_) => panic!("manager not released"),
        };

        // todo:
        // 1. send data over raw chmux channel
        // 2. open queued ports

        // todo: provide feedback in chmux when connect request has been sent
        // but that is not what we want
        // actually we want feedback when data has been sent
        // but this could also easily be implemented, but would delay things maybe
        // hmm, not good
        // would be nice to find a way around this
        // so todo: find a way to ensure that a connect request is queued after data
        

        Ok(())
    }
}

struct RemoteReceiver<T> {
    receiver: chmux::RawReceiver,
    _phantom: PhantomData<T>,
}

impl<T> RemoteReceiver<T>
where
    T: DeserializeOwned,
{
    pub async fn recv(&mut self) -> Result<Option<T>, BaseReceiveError> {
        let data = self.receiver.recv().await?;

        // we now need to deserialize

        Ok(item)
    }

    pub async fn close(&mut self) {}
}

enum ReceiverSource<T> {
    Local(tokio::sync::mpsc::Receiver<T>),
    Remote(RemoteReceiver<T>),
}

impl<T> ReceiverSource<T>
where
    T: DeserializeOwned,
{
    pub async fn recv(&mut self) -> Result<Option<T>, BaseReceiveError> {
        let item = match self {
            Self::Local(r) => r.recv().await,
            Self::Remote(r) => r.recv().await?,
        };
        Ok(item)
    }

    pub async fn close(&mut self) {
        match self {
            Self::Local(r) => r.close(),
            Self::Remote(r) => r.close().await,
        }
    }
}

// Now how would the sender handle this?
// but we still need a way to rebuild channel after reception.
// so how to do that?
// rebuild is only necessary for remote channels

pub struct BaseReceiver<T> {
    source: ReceiverSource<T>,
    source_queue: tokio::sync::mpsc::UnboundedReceiver<ReceiverSource<T>>,
}

impl<T> BaseReceiver<T>
where
    T: DeserializeOwned + Send + 'static,
{
    pub async fn recv(&mut self) -> Result<Option<T>, BaseReceiveError> {
        loop {
            match self.source.recv().await? {
                Some(item) => return Ok(Some(item)),
                None => match self.source_queue.recv().await {
                    Some(next_source) => self.source = next_source,
                    None => return Ok(None),
                },
            }
        }
    }

    pub async fn close(&mut self) {
        self.source.close().await;

        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        mem::swap(&mut rx, &mut self.source_queue);
        tokio::spawn(async move {
            while let Some(mut source) = rx.recv().await {
                source.close().await;
                if tx.send(source).is_err() {
                    break;
                }
            }
        });
    }
}

pub fn base_channel<T>(buffer: usize) -> (BaseSender<T>, BaseReceiver<T>)
where
    T: Serialize + DeserializeOwned,
{
    let (sender, receiver) = mpsc::channel(buffer);
    let base_sender = BaseSender { sender, _phantom: PhantomData };
    let base_receiver = BaseReceiver { receiver, _phantom: PhantomData };
    (base_sender, base_receiver)
}

#[derive(Serialize)]
pub struct MyStruct {
    a: BaseSender<()>,
}
