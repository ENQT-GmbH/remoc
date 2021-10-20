//! Multi producer single customer remote channel.
//!
//! The sender and receiver can both be sent to remote endpoints.
//! The channel also works if both halves are local.
//! Forwarding over multiple connections is supported.
//!
//! This has similar functionality as [tokio::sync::mpsc] with the additional
//! ability to work over remote connections.
//!
//! # Example
//!
//! In the following example the client sends a number and an MPSC channel sender to the server.
//! The server counts to the number and sends each value to the client over the MPSC channel.
//!
//! ```
//! use remoc::prelude::*;
//!
//! #[derive(Debug, serde::Serialize, serde::Deserialize)]
//! struct CountReq {
//!     up_to: u32,
//!     seq_tx: rch::mpsc::Sender<u32>,
//! }
//!
//! // This would be run on the client.
//! async fn client(mut tx: rch::base::Sender<CountReq>) {
//!     let (seq_tx, mut seq_rx) = rch::mpsc::channel(1);
//!     tx.send(CountReq { up_to: 4, seq_tx }).await.unwrap();
//!
//!     assert_eq!(seq_rx.recv().await.unwrap(), Some(0));
//!     assert_eq!(seq_rx.recv().await.unwrap(), Some(1));
//!     assert_eq!(seq_rx.recv().await.unwrap(), Some(2));
//!     assert_eq!(seq_rx.recv().await.unwrap(), Some(3));
//!     assert_eq!(seq_rx.recv().await.unwrap(), None);
//! }
//!
//! // This would be run on the server.
//! async fn server(mut rx: rch::base::Receiver<CountReq>) {
//!     while let Some(CountReq { up_to, seq_tx }) = rx.recv().await.unwrap() {
//!         for i in 0..up_to {
//!             seq_tx.send(i).await.unwrap();
//!         }
//!     }
//! }
//! # tokio_test::block_on(remoc::doctest::client_server(client, server));
//! ```
//!

use crate::RemoteSend;

mod distributor;
mod receiver;
mod sender;

pub use distributor::{DistributedReceiverHandle, Distributor};
pub use receiver::{Receiver, RecvError};
pub use sender::{Permit, SendError, Sender, TrySendError};

/// Creates a bounded channel for communicating between asynchronous tasks with back pressure.
///
/// The sender and receiver may be sent to remote endpoints via channels.
pub fn channel<T, Codec>(local_buffer: usize) -> (Sender<T, Codec>, Receiver<T, Codec>)
where
    T: RemoteSend,
{
    assert!(local_buffer > 0, "local_buffer must not be zero");

    let (tx, rx) = tokio::sync::mpsc::channel(local_buffer);
    let (closed_tx, closed_rx) = tokio::sync::watch::channel(false);
    let (remote_send_err_tx, remote_send_err_rx) = tokio::sync::watch::channel(None);

    let sender = Sender::new(tx, closed_rx, remote_send_err_rx);
    let receiver = Receiver::new(rx, closed_tx, remote_send_err_tx);
    (sender, receiver)
}
