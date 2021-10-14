//! A single-producer, multi-consumer channel that only retains the last sent value.

use std::{fmt, ops::Deref};

use crate::RemoteSend;

mod receiver;
mod sender;

pub use receiver::{Receiver, RecvError, TransportedReceiver};
pub use sender::{SendError, Sender, TransportedSender};

/// Length of queuing for storing errors that occured during remote send.
const ERROR_QUEUE: usize = 16;

/// Returns a reference to the inner value.
pub struct Ref<'a, T>(tokio::sync::watch::Ref<'a, Result<T, RecvError>>);

impl<'a, T> Deref for Ref<'a, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        self.0.as_ref().unwrap()
    }
}

impl<'a, T> fmt::Debug for Ref<'a, T>
where
    T: fmt::Debug,
{
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{:?}", &*self)
    }
}

/// Creates a new watch channel, returning the sender and receiver.
///
/// The sender and receiver may be sent to remote endpoints via channels.
pub fn channel<T, Codec>(init: T) -> (Sender<T, Codec>, Receiver<T, Codec>)
where
    T: RemoteSend,
{
    let (tx, rx) = tokio::sync::watch::channel(Ok(init));
    let (remote_send_err_tx, remote_send_err_rx) = tokio::sync::mpsc::channel(ERROR_QUEUE);

    let sender = Sender::new(tx, remote_send_err_tx.clone(), remote_send_err_rx);
    let receiver = Receiver::new(rx, remote_send_err_tx);
    (sender, receiver)
}
