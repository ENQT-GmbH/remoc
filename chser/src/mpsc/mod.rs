mod receiver;
mod sender;

const BACKCHANNEL_MSG_CLOSE: u8 = 0x01;
const BACKCHANNEL_MSG_ERROR: u8 = 0x02;

use std::{marker::PhantomData, sync::Mutex};

pub use receiver::{ReceiveError, Receiver, TransportedReceiver};
pub use sender::{SendError, Sender, TransportedSender};

use receiver::ReceiverInner;

/// Creates a bounded channel for communicating between asynchronous tasks with backpressure.
///
/// The sender and receiver may be sent to remote endpoints via channels.
pub fn channel<T, Codec, const SEND_BUFFER: usize, const RECEIVE_BUFFER: usize>(
    local_buffer: usize,
) -> (Sender<T, Codec, SEND_BUFFER>, Receiver<T, Codec, RECEIVE_BUFFER>) {
    assert!(SEND_BUFFER > 0, "SEND_BUFFER must not be zero");
    assert!(RECEIVE_BUFFER > 0, "RECEIVE_BUFFER must not be zero");
    assert!(local_buffer > 0, "local_buffer must not be zero");

    let (tx, rx) = tokio::sync::mpsc::channel(local_buffer);
    let (closed_tx, closed_rx) = tokio::sync::watch::channel(false);
    let (remote_send_err_tx, remote_send_err_rx) = tokio::sync::watch::channel(None);

    let sender = Sender { tx, closed_rx, remote_send_err_rx, _codec: PhantomData };

    let receiver = Receiver {
        inner: Some(ReceiverInner { rx, closed_tx, remote_send_err_tx, _codec: PhantomData }),
        successor_tx: Mutex::new(None),
    };

    (sender, receiver)
}
