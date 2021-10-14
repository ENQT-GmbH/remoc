//! A one-shot channel is used for sending a single message between asynchronous tasks.

use serde::{de::DeserializeOwned, Serialize};

use super::mpsc;
use crate::codec;

mod receiver;
mod sender;

pub use receiver::{Receiver, RecvError, TryRecvError};
pub use sender::{SendError, Sender};

/// Create a new one-shot channel for sending single values across asynchronous tasks.
///
/// The sender and receiver may be sent to remote endpoints via channels.
pub fn channel<T, Codec>() -> (Sender<T, Codec>, Receiver<T, Codec>)
where
    T: Serialize + DeserializeOwned + Send + 'static,
    Codec: codec::Codec,
{
    let (tx, rx) = mpsc::channel(1);
    let tx = tx.set_buffer();
    let rx = rx.set_buffer();
    (Sender(tx), Receiver(rx))
}
