//! A multi-producer, multi-consumer broadcast queue. Each sent value is seen by all consumers.

use serde::{Deserialize, Serialize};

use super::RemoteSend;
use crate::codec::CodecT;

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
pub fn channel<T, Codec, const RECV_BUFFER: usize>(
    send_buffer: usize,
) -> (Sender<T, Codec>, Receiver<T, Codec, RECV_BUFFER>)
where
    T: RemoteSend + Clone,
    Codec: CodecT,
{
    let sender = Sender::new();
    let receiver = sender.subscribe(send_buffer);
    (sender, receiver)
}
