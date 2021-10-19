//! A one-shot channel is used for sending a single message between asynchronous, remote tasks.
//!
//! The sender and receiver can both be sent to remote endpoints.
//! The channel also works if both halves are local.
//! Forwarding over multiple connections is supported.
//!
//! This has similar functionality as [tokio::sync::oneshot] with the additional
//! ability to work over remote connections.
//!
//! # Example
//!
//! In the following example the client sends a number and a oneshot channel sender to the server.
//! The server squares the received number and sends the result back over the oneshot channel.
//!
//! ```
//! use remoc::prelude::*;
//!
//! #[derive(Debug, serde::Serialize, serde::Deserialize)]
//! struct SquareReq {
//!     number: u32,
//!     result_tx: rch::oneshot::Sender<u32>,
//! }
//!
//! // This would be run on the client.
//! async fn client(mut tx: rch::base::Sender<SquareReq>) {
//!     let (result_tx, result_rx) = rch::oneshot::channel();
//!     tx.send(SquareReq { number: 4, result_tx }).await.unwrap();
//!     let result = result_rx.await.unwrap();
//!     assert_eq!(result, 16);
//! }
//!
//! // This would be run on the server.
//! async fn server(mut rx: rch::base::Receiver<SquareReq>) {
//!     while let Some(req) = rx.recv().await.unwrap() {
//!         req.result_tx.send(req.number * req.number).unwrap();
//!     }
//! }
//! # tokio_test::block_on(remoc::doctest::client_server(client, server));
//! ```
//!

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
