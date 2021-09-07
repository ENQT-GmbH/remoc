//! Raw chmux channel.

use std::{
    fmt,
    sync::{Arc, Mutex},
};

mod receiver;
mod sender;

pub use receiver::{Receiver, TransportedReceiver};
pub use sender::{Sender, TransportedSender};

use crate::interlock::{Interlock, Location};

#[derive(Debug, Clone)]
pub enum ConnectError {
    /// The corresponding sender or receiver has been dropped.
    Dropped,
    /// Error initiating chmux connection.
    Connect(chmux::ConnectError),
    /// Error accepting chmux connection.
    Accept(chmux::ListenerError),
}

impl fmt::Display for ConnectError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Self::Dropped => write!(f, "corresponding sender or receiver has been dropped"),
            Self::Connect(err) => write!(f, "chmux connect error: {}", err),
            Self::Accept(err) => write!(f, "chmux accept error: {}", err),
        }
    }
}

impl std::error::Error for ConnectError {}

/// Creates a new chmux channel that is established by sending either the sender or receiver
/// over a remote channel.
pub fn channel() -> (Sender, Receiver) {
    let (sender_tx, sender_rx) = tokio::sync::mpsc::unbounded_channel();
    let (receiver_tx, receiver_rx) = tokio::sync::mpsc::unbounded_channel();
    let interlock = Arc::new(Mutex::new(Interlock { sender: Location::Local, receiver: Location::Local }));

    let sender = Sender { sender: None, sender_rx, receiver_tx: Some(receiver_tx), interlock: interlock.clone() };
    let receiver = Receiver { receiver: None, sender_tx: Some(sender_tx), receiver_rx, interlock };
    (sender, receiver)
}
