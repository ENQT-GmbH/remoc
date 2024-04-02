//! Channel data forwarding.

use bytes::Buf;
use std::{fmt, num::Wrapping};

use super::{ConnectError, PortReq, Received, RecvChunkError, RecvError, SendError};

/// An error occurred during forwarding of a message.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum ForwardError {
    /// Sending failed.
    Send(SendError),
    /// Receiving failed.
    Recv(RecvError),
}

impl From<SendError> for ForwardError {
    fn from(err: SendError) -> Self {
        Self::Send(err)
    }
}

impl From<RecvError> for ForwardError {
    fn from(err: RecvError) -> Self {
        Self::Recv(err)
    }
}

impl fmt::Display for ForwardError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            ForwardError::Send(err) => write!(f, "forward send failed: {err}"),
            ForwardError::Recv(err) => write!(f, "forward receive failed: {err}"),
        }
    }
}

impl std::error::Error for ForwardError {}

/// Forwards all data received from a receiver to a sender.
///
/// This also recursively setups forwarding for all transmitted ports.
///
/// Returns the total number of bytes forwarded.
pub async fn forward(rx: &mut super::Receiver, tx: &mut super::Sender) -> Result<usize, ForwardError> {
    // Required to avoid borrow checking loop limitation.
    fn spawn_forward(id: u32, mut rx: super::Receiver, mut tx: super::Sender) {
        tokio::spawn(async move {
            if let Err(err) = forward(&mut rx, &mut tx).await {
                tracing::debug!("port forwarding for id {id} failed: {err}");
            }
        });
    }

    let mut total = Wrapping(0);

    loop {
        match rx.recv_any().await? {
            // Data.
            Some(Received::Data(data)) => {
                total += data.remaining();
                tx.send(data.into()).await?;
            }

            // Data chunks.
            Some(Received::Chunks) => {
                let mut chunk_tx = tx.send_chunks();
                loop {
                    match rx.recv_chunk().await {
                        Ok(Some(chunk)) => {
                            total += chunk.remaining();
                            chunk_tx = chunk_tx.send(chunk).await?;
                        }
                        Ok(None) => {
                            chunk_tx.finish().await?;
                            break;
                        }
                        Err(RecvChunkError::Cancelled) => break,
                        Err(RecvChunkError::ChMux) => return Err(ForwardError::Recv(RecvError::ChMux)),
                    }
                }
            }

            // Ports.
            Some(Received::Requests(reqs)) => {
                let allocator = tx.port_allocator();

                // Allocate local outgoing ports for forwarding.
                let mut ports = Vec::new();
                let mut wait = false;
                for req in &reqs {
                    let port = allocator.allocate().await;
                    ports.push(PortReq::new(port).with_id(req.id()));
                    wait |= req.is_wait();
                }

                // Connect them.
                let connects = tx.connect(ports, wait).await?;
                for (req, connect) in reqs.into_iter().zip(connects) {
                    tokio::spawn(async move {
                        let id = req.id();
                        match connect.await {
                            Ok((out_tx, out_rx)) => {
                                let in_port = out_tx.port_allocator().allocate().await;
                                match req.accept_from(in_port).await {
                                    Ok((in_tx, in_rx)) => {
                                        spawn_forward(id, out_rx, in_tx);
                                        spawn_forward(id, in_rx, out_tx);
                                    }
                                    Err(err) => {
                                        tracing::debug!("port forwarding for id {id} failed to accept: {err}");
                                    }
                                }
                            }
                            Err(err) => {
                                tracing::debug!("port forwarding for id {id} failed to connect: {err}");
                                req.reject(matches!(
                                    err,
                                    ConnectError::LocalPortsExhausted | ConnectError::RemotePortsExhausted
                                ))
                                .await;
                            }
                        }
                    });
                }
            }

            // End.
            None => break,
        }
    }

    Ok(total.0)
}
