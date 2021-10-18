//! A channel that exchanges values of arbitrary type with a remote endpoint and
//! is established by sending exactly one half of it over an existing channel.
//!
//! This is a lighter and more restricted version of an [MPSC channel](super::mpsc).
//! It does not spawn a task per connected channel, but forwarding is not supported
//! and exactly one half of the channel must be sent to a remote endpoint.

use std::sync::{Arc, Mutex};

mod receiver;
mod sender;

use super::interlock::{Interlock, Location};
pub use receiver::{Receiver, RecvError};
pub use sender::{SendError, SendErrorKind, Sender};

/// Creates a new local/remote channel that is established by sending either the sender or receiver
/// over a remote channel.
pub fn channel<T, Codec>() -> (Sender<T, Codec>, Receiver<T, Codec>) {
    let (sender_tx, sender_rx) = tokio::sync::mpsc::unbounded_channel();
    let (receiver_tx, receiver_rx) = tokio::sync::mpsc::unbounded_channel();
    let interlock = Arc::new(Mutex::new(Interlock { sender: Location::Local, receiver: Location::Local }));

    let sender = Sender { sender: None, sender_rx, receiver_tx: Some(receiver_tx), interlock: interlock.clone() };
    let receiver = Receiver { receiver: None, sender_tx: Some(sender_tx), receiver_rx, interlock };
    (sender, receiver)
}
