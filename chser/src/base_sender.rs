use crate::{Obtainer, base_receiver::ReceiverSource, codec::{CodecT, DeserializationError, SerializationError}, remote_receiver::{PortDeserializer, ReceiveError, RemoteReceiver}, remote_sender::{ObtainSenderError, PortSerializer, RemoteSender, SendError, SendErrorKind}};
use std::{
    any::Any,
    cell::{Cell, RefCell},
    collections::HashMap,
    error::Error,
    marker::PhantomData,
    mem,
    rc::{Rc, Weak},
    sync::Arc,
};

use chmux::{Connect, DataBuf, ListenerError, PortAllocator, PortNumber, RawReceiver, Received, Request};
use futures::{future::BoxFuture, Future, FutureExt};
use serde::{de::DeserializeOwned, ser, Deserialize, Serialize};
use tokio::sync::{oneshot, Mutex};


/// Target for sending.
pub enum SenderTarget<T, Codec> {
    Local {
        tx: tokio::sync::mpsc::Sender<T>,
        source_tx: tokio::sync::mpsc::UnboundedSender<tokio::sync::oneshot::Receiver<ReceiverSource<T, Codec>>>
    },
    Remote(RemoteSender<T, Codec>),
    Closed,
}

/// Sender in serialized form for transport.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransportedBaseSender<T> {
    /// Chmux port of sending endpoint.
    /// None if channel has been closed.
    pub port: Option<u32>,
    /// Data type.
    _phantom: PhantomData<T>,
}

/// Sender that can send to local or remote endpoint.
pub struct BaseSender<T, Codec> {
    target: SenderTarget<T, Codec>,
}

impl<T, Codec> BaseSender<T, Codec>
where
    T: Serialize,
    Codec: CodecT,
{
    /// Sends an item.
    pub async fn send(&mut self, item: T) -> Result<(), SendError<T>> {
        match &mut self.target {
            SenderTarget::Local {tx, ..} => {
                if let Err(tokio::sync::mpsc::error::SendError(item)) = tx.send(item).await {
                    return Err(SendError::new(SendErrorKind::Closed, item));
                }
            }
            SenderTarget::Remote(tx) => {
                tx.send(item).await?;
            }
            SenderTarget::Closed => return Err(SendError::new(SendErrorKind::Closed, item)),
        }
        Ok(())
    }
}

impl<T, Codec> Serialize for BaseSender<T, Codec>
where
    T: Serialize + DeserializeOwned + Send + 'static,
    Codec: CodecT,
{
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        // Problem is similar here.
        // Target may be changing.
        // We could proc
        let port = match &self.target {
            SenderTarget::Local {source_tx, ..} => {
                let (tx, rx) = oneshot::channel();
                match source_tx.send(rx) {
                    Ok(()) => {
                        // Receiver is open.
                        let port = PortSerializer::connect(|connect, allocator| {
                            async move {
                                tx.send(ReceiverSource::Remote(RemoteReceiver::new(
                                    Obtainer::new(async move {
                                        let (_, receiver) = connect.await?;
                                        Ok(receiver)
                                    }),
                                    allocator,
                                )));
                            }
                            .boxed()
                        })?;
                        Some(port)
                    }
                    Err(_) => {
                        // Receiver has been closed
                        None
                    }
                }
            }
            SenderTarget::Remote(remote_tx) => {
                // TODO: message forwarding pump
                todo!()
            }
            SenderTarget::Closed => {
                None
            }
        };

        let transported = TransportedBaseSender::<T> { port, _phantom: PhantomData };
        transported.serialize(serializer)
    }
}

impl<'de, T, Codec> Deserialize<'de> for BaseSender<T, Codec>
where
    T: Serialize + DeserializeOwned + Send,
    Codec: CodecT,
{
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        // Deserialize transported sender to get remote port number of port request.
        let transported = TransportedBaseSender::<T>::deserialize(deserializer)?;

        match transported.port {
            Some(port) => {
                // Register remote port number with port gatherer and forward created raw sender.
                let (request_tx, request_rx) = oneshot::channel();
                let (allocator, local_port) = PortDeserializer::accept(port, |request| {
                    let _ = request_tx.send(request);
                })?;
                let obtain = Obtainer::new(async move {
                    let request = request_rx.await.map_err(|_| ObtainSenderError::Dropped)?;
                    let (sender, _) = request.accept().await?;
                    Ok(sender)
                });

                Ok(Self { target: SenderTarget::Remote(RemoteSender::new(obtain, allocator)) })
            }
            None => Ok(Self { target: SenderTarget::Closed })
        }
    }
}
