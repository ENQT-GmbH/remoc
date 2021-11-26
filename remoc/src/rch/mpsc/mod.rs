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
pub use receiver::{Receiver, RecvError, TryRecvError};
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
    let (closed_tx, closed_rx) = tokio::sync::watch::channel(None);
    let (remote_send_err_tx, remote_send_err_rx) = tokio::sync::watch::channel(None);

    let sender = Sender::new(tx, closed_rx, remote_send_err_rx);
    let receiver = Receiver::new(rx, closed_tx, false, remote_send_err_tx);
    (sender, receiver)
}

/// Send implementation for deserializer of Sender and serializer of Receiver.
macro_rules! send_impl {
    ($T:ty, $rx:ident, $raw_tx:ident, $raw_rx:ident, $remote_send_err_tx:ident, $closed_tx:ident) => {
        // Encode data using remote sender.
        let mut remote_tx = base::Sender::<Result<$T, RecvError>, Codec>::new($raw_tx);

        // Process events.
        loop {
            tokio::select! {
                biased;

                // Back channel message from remote endpoint.
                backchannel_msg = $raw_rx.recv() => {
                    match backchannel_msg {
                        Ok(Some(mut msg)) if msg.remaining() >= 1 => {
                            match msg.get_u8() {
                                BACKCHANNEL_MSG_CLOSE => {
                                    let _ = $remote_send_err_tx.send(Some(RemoteSendError::Closed));
                                    let _ = $closed_tx.send(Some(ClosedReason::Closed));
                                    break;
                                }
                                BACKCHANNEL_MSG_ERROR => {
                                    let _ = $remote_send_err_tx.send(Some(RemoteSendError::Forward));
                                    let _ = $closed_tx.send(Some(ClosedReason::Failed));
                                    break;
                                }
                                _ => (),
                            }
                        },
                        Ok(Some(_)) => (),
                        Ok(None) => {
                            let _ = $remote_send_err_tx.send(Some(RemoteSendError::Send(
                                base::SendErrorKind::Send(chmux::SendError::Closed { gracefully: false })
                            )));
                            let _ = $closed_tx.send(Some(ClosedReason::Dropped));
                            break;
                        }
                        _ => {
                            let _ = $remote_send_err_tx.send(Some(RemoteSendError::Send(
                                base::SendErrorKind::Send(chmux::SendError::ChMux)
                            )));
                            let _ = $closed_tx.send(Some(ClosedReason::Failed));
                            break;
                        },
                    }
                }

                // Data to send to remote endpoint.
                value_opt = $rx.recv() => {
                    match value_opt {
                        Some(value) => {
                            if let Err(err) = remote_tx.send(value).await {
                                let _ = $remote_send_err_tx.send(Some(RemoteSendError::Send(err.kind)));
                                let _ = $closed_tx.send(Some(ClosedReason::Failed));
                            }
                        }
                        None => break,
                    }
                }
            }
        }
    };
}
pub(crate) use send_impl;

/// Receive implementation for serializer of Sender and deserializer of Receiver.
macro_rules! recv_impl {
    ($T:ty, $tx:ident, $raw_tx:ident, $raw_rx:ident, $remote_send_err_rx:ident, $closed_rx:ident) => {
        // Decode raw received data using remote receiver.
        let mut remote_rx = base::Receiver::<Result<$T, RecvError>, Codec>::new($raw_rx);

        // Process events.
        loop {
            tokio::select! {
                biased;

                // Channel closure requested locally.
                res = $closed_rx.changed() => {
                    match res {
                        Ok(()) => {
                            let reason = $closed_rx.borrow().clone();
                            match reason {
                                Some(ClosedReason::Closed) => {
                                    let _ = $raw_tx.send(vec![BACKCHANNEL_MSG_CLOSE].into()).await;
                                }
                                Some(ClosedReason::Dropped) => break,
                                Some(ClosedReason::Failed) => {
                                    let _ = $raw_tx.send(vec![BACKCHANNEL_MSG_ERROR].into()).await;
                                }
                                None => (),
                            }
                        },
                        Err(_) => break,
                    }
                }

                // Notify remote endpoint of error.
                Ok(()) = $remote_send_err_rx.changed() => {
                    if $remote_send_err_rx.borrow().as_ref().is_some() {
                        let _ = $raw_tx.send(vec![BACKCHANNEL_MSG_ERROR].into()).await;
                    }
                }

                // Data received from remote endpoint.
                res = remote_rx.recv() => {
                    let mut is_final_err = false;
                    let value = match res {
                        Ok(Some(value)) => value,
                        Ok(None) => break,
                        Err(err) => {
                            is_final_err = err.is_final();
                            Err(RecvError::RemoteReceive(err))
                        },
                    };
                    if $tx.send(value).await.is_err() {
                        break;
                    }
                    if is_final_err {
                        break;
                    }
                }
            }
        }
    };
}
pub(crate) use recv_impl;
