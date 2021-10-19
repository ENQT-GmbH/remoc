//! A single-producer, multi-consumer remote channel that only retains the last sent value.
//!
//! The sender and receiver can both be sent to remote endpoints.
//! The channel also works if both halves are local.
//! Forwarding over multiple connections is supported.
//!
//! This has similar functionality as [tokio::sync::watch] with the additional
//! ability to work over remote connections.
//!
//! # Alternatives
//!
//! If your endpoints need the ability to change the value and synchronize the changes
//! with other endpoints, consider using an [read/write lock](crate::robj::rw_lock)
//! instead.
//!
//! # Example
//!
//! In the following example the client sends a number and a watch channel sender to the server.
//! The server counts to the number and sends each value to the client over the watch channel.
//!
//! ```
//! use remoc::prelude::*;
//!
//! #[derive(Debug, serde::Serialize, serde::Deserialize)]
//! struct CountReq {
//!     up_to: u32,
//!     watch_tx: rch::watch::Sender<u32>,
//! }
//!
//! // This would be run on the client.
//! async fn client(mut tx: rch::base::Sender<CountReq>) {
//!     let (watch_tx, mut watch_rx) = rch::watch::channel(0);
//!     tx.send(CountReq { up_to: 4, watch_tx }).await.unwrap();
//!
//!     // Intermediate values may be missed.
//!     while *watch_rx.borrow_and_update().unwrap() != 3 {
//!         watch_rx.changed().await;
//!     }
//! }
//!
//! // This would be run on the server.
//! async fn server(mut rx: rch::base::Receiver<CountReq>) {
//!     while let Some(CountReq { up_to, watch_tx }) = rx.recv().await.unwrap() {
//!         for i in 0..up_to {
//!             watch_tx.send(i).unwrap();
//!         }
//!     }
//! }
//! # tokio_test::block_on(remoc::doctest::client_server(client, server));
//! ```
//!

use std::{fmt, ops::Deref};

use crate::RemoteSend;

mod receiver;
mod sender;

pub use receiver::{Receiver, RecvError};
pub use sender::{SendError, Sender};

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
