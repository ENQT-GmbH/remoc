//! A multi-producer, multi-consumer broadcast queue with receivers that may be located on remote endpoints.
//!
//! Each sent value is seen by all consumers.
//! The senders must be local, while the receivers can be sent to
//! remote endpoints.
//! Forwarding is supported.
//!
//! This has similar functionality as [tokio::sync::broadcast] with the additional
//! ability to work over remote connections.

use serde::{Deserialize, Serialize};

use super::buffer;
use crate::{codec, RemoteSend};

mod receiver;
mod sender;

pub use receiver::{Receiver, RecvError};
pub use sender::{SendError, Sender};

/// Broadcast transport message.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum BroadcastMsg<T> {
    /// Value.
    Value(T),
    /// Lagged notification.
    Lagged,
}

/// Create a bounded, multi-producer, multi-consumer channel where each sent value is broadcasted to all active receivers.
pub fn channel<T, Codec, ReceiveBuffer>(
    send_buffer: usize,
) -> (Sender<T, Codec>, Receiver<T, Codec, ReceiveBuffer>)
where
    T: RemoteSend + Clone,
    Codec: codec::Codec,
    ReceiveBuffer: buffer::Size,
{
    let sender = Sender::new();
    let receiver = sender.subscribe(send_buffer);
    (sender, receiver)
}
