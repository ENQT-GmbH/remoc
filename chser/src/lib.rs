use std::cell::{Cell, RefCell};
use std::error::Error;
use std::marker::PhantomData;
use std::rc::{Rc, Weak};

use serde::de::DeserializeOwned;
use serde::{Serialize, ser};

pub mod mpsc;


pub struct Distributor {
    client: chmux::RawClient,
    listener: chmux::RawListener,

}

enum DistributorReq {
    SendBasePort ()
}

impl Distributor {
    pub fn new<S, R>(client: chmux::RawClient, listener: chmux::RawListener) -> (Self, mpsc::Sender<S>, mpsc::Receiver<R>) {
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
        let this = Self {client, listener};

        // the sender and receiver must be connected. 
        todo!()
    }

    pub async fn run(self) -> Result<(), ()> {
        // how will we connect to a channel?
        // it will be send over us

    }
}

thread_local! {
    static MANAGER: RefCell<Weak<RefCell<ChManager>>> = RefCell::new(Weak::new());
}




struct ChManager {

}

pub enum SendError {
    Closed,
    Serialize(Box<dyn Error>),
}

pub struct BaseSender<T> {
    sender: mpsc::Sender<Vec<u8>>,
    _phantom: PhantomData<T>,
}

impl<T: Serialize> BaseSender<T> {
    pub async fn send(&self, item: &T) -> Result<(), SendError> {
        // 1. serialize
        // 2. send

        let manager = Rc::new(RefCell::new(ChManager{}));
        let manager_weak = Rc::downgrade(&manager);
        MANAGER.with(|m| m.replace(manager_weak));
        //MANAGER.replace(manager);

        let data = serde_json::to_vec(item).map_err(|err| SendError::Serialize(Box::new(err)))?;

        let manager = match Rc::try_unwrap(manager) {
            Ok(manager) => manager,
            Err(_) => panic!("manager not released"),
        };
        
        // So it would be possible to write a forwarding serializer.
        // => no problem
        // Now once we arrive at the point that the forwarding serializer
        // reaches our BaseSender, what to we do?
        //
        // The BaseSender will be a member of a parent struct
        // so it will be passed into the serializer?
        //
        // Ok so we have to see if we can detect it.
        // Could we implement a trait with default methods for everything that is serialize?
        //
        // But we will be called with a serializer, so let's hack that together.

        // now we need to inject the channel behaviour.
        // so what are the options?
        // I cannot use any because there is no trait bound on S.

        self.sender.send(data).await.map_err(|_| SendError::Closed)?;
        Ok(())
    }
}

impl<T> Serialize for BaseSender<T> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let manager = match MANAGER.with(|m| m.borrow().upgrade()) {
            Some(manager) => manager,
            None => return Err(ser::Error::custom("channel manager missing"))
        };

        let manager = manager.borrow_mut();
        // So what does it need to do here?

    }
}

#[derive(Serialize)]
pub struct MyStruct {
    a: BaseSender<()>,
}

pub struct BaseReceiver<T> {
    receiver: mpsc::Receiver<Vec<u8>>,
    _phantom: PhantomData<T>,
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
