//! A one-shot channel is used for sending a single message between asynchronous, remote tasks.
//!
//! The sender and receiver can both be sent to remote endpoints.
//! The channel also works if both halves are local.
//! Forwarding over multiple connections is supported.
//!
//! This has similar functionality as [tokio::sync::oneshot] with the additional
//! ability to work over remote connections.

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
