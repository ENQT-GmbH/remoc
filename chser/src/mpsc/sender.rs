use bytes::Buf;
use futures::FutureExt;
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use std::{
    marker::PhantomData,
    sync::{Arc, Weak},
};

use crate::{
    codec::CodecT,
    mpsc::{BACKCHANNEL_MSG_CLOSE, BACKCHANNEL_MSG_ERROR},
    remote::{self, Obtainer, PortDeserializer, PortSerializer, SendErrorKind},
};

pub enum SendError<T> {
    Closed(T),
    Remote(remote::SendErrorKind),
}

/// Send values to the associated [Receiver], which may be located on a remote endpoint.
///
/// Instances are created by the [channel] function.
pub struct Sender<T, Codec, const BUFFER: usize> {
    tx: Weak<tokio::sync::mpsc::Sender<Result<T, remote::ReceiveError>>>,
    closed_rx: tokio::sync::watch::Receiver<bool>,
    remote_send_err_rx: tokio::sync::watch::Receiver<Option<remote::SendErrorKind>>,
    _dropped_tx: tokio::sync::oneshot::Sender<()>,
    _codec: PhantomData<Codec>,
}

#[derive(Serialize, Deserialize)]
pub struct TransportedSender<T, Codec> {
    /// chmux port number. `None` if closed.
    port: Option<u32>,
    /// Data type.
    data: PhantomData<T>,
    /// Data codec.
    codec: PhantomData<Codec>,
}

impl<T, Codec, const BUFFER: usize> Sender<T, Codec, BUFFER>
where
    T: Send + 'static,
{
    /// Creates a new sender.
    pub(crate) fn new(
        tx: tokio::sync::mpsc::Sender<Result<T, remote::ReceiveError>>,
        mut closed_rx: tokio::sync::watch::Receiver<bool>,
        remote_send_err_rx: tokio::sync::watch::Receiver<Option<remote::SendErrorKind>>,
    ) -> Self {
        let tx = Arc::new(tx);
        let (dropped_tx, mut dropped_rx) = tokio::sync::oneshot::channel();

        let this = Self {
            tx: Arc::downgrade(&tx),
            closed_rx: closed_rx.clone(),
            remote_send_err_rx,
            _dropped_tx: dropped_tx,
            _codec: PhantomData,
        };

        // Drop strong reference to sender when channel is closed.
        tokio::spawn(async move {
            loop {
                tokio::select! {
                    _ = closed_rx.changed() => {
                        if *closed_rx.borrow() {
                            break;
                        }
                    },
                    _ = &mut dropped_rx => break,
                }
            }

            drop(tx);
        });

        this
    }

    /// Creates a new sender that is closed.
    pub(crate) fn new_closed() -> Self {
        Self {
            tx: Weak::new(),
            closed_rx: tokio::sync::watch::channel(true).1,
            remote_send_err_rx: tokio::sync::watch::channel(None).1,
            _dropped_tx: tokio::sync::oneshot::channel().0,
            _codec: PhantomData,
        }
    }

    /// Sends a value over this channel.
    pub async fn send(&self, value: T) -> Result<(), SendError<T>> {
        if let Some(err) = self.remote_send_err_rx.borrow().as_ref() {
            return Err(SendError::Remote(err.clone()));
        }

        if let Some(tx) = self.tx.upgrade() {
            if let Err(err) = tx.send(Ok(value)).await {
                return Err(SendError::Closed(err.0.unwrap()));
            }
        } else {
            return Err(SendError::Closed(value));
        }

        Ok(())
    }

    /// Completes when the receiver has been closed or dropped.
    pub async fn closed(&self) {
        let mut closed = self.closed_rx.clone();
        while !*closed.borrow() {
            if closed.changed().await.is_err() {
                break;
            }
        }
    }

    /// Returns whether the receiver has been closed or dropped.
    pub fn is_closed(&self) -> bool {
        *self.closed_rx.borrow()
    }
}

impl<T, Codec, const BUFFER: usize> Drop for Sender<T, Codec, BUFFER> {
    fn drop(&mut self) {
        // empty
    }
}

impl<T, Codec, const BUFFER: usize> Serialize for Sender<T, Codec, BUFFER>
where
    T: Serialize + DeserializeOwned + Send + 'static,
    Codec: CodecT,
{
    /// Serializes this sender for sending over a chmux channel.
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let port = match self.tx.upgrade() {
            // Channel is open.
            Some(tx) => {
                // Prepare channel for takeover.
                let mut closed_rx = self.closed_rx.clone();
                let mut remote_send_err_rx = self.remote_send_err_rx.clone();

                Some(PortSerializer::connect(|connect, allocator| {
                    tokio::spawn(async move {
                        // Establish chmux channel.
                        let (mut raw_tx, raw_rx) = match connect.await {
                            Ok(tx_rx) => tx_rx,
                            Err(err) => {
                                let _ = tx.send(Err(remote::ReceiveError::Connect(err))).await;
                                return;
                            }
                        };

                        // Decode raw received data using remote receiver.
                        let mut remote_rx = remote::Receiver::<Result<T, remote::ReceiveError>, Codec>::new(
                            Obtainer::ready(Ok(raw_rx)),
                            allocator,
                        );

                        // Process events.
                        let mut close_sent = false;
                        loop {
                            tokio::select! {
                                biased;

                                // Channel closure requested locally.
                                res = closed_rx.changed() => {
                                    match res {
                                        Ok(()) if *closed_rx.borrow() && !close_sent => {
                                            let _ = raw_tx.send(vec![BACKCHANNEL_MSG_CLOSE].into()).await;
                                            close_sent = true;
                                        }
                                        Ok(()) => (),
                                        Err(_) => break,
                                    }
                                }

                                // Notify remote endpoint of error.
                                Ok(()) = remote_send_err_rx.changed() => {
                                    if remote_send_err_rx.borrow().as_ref().is_some() {
                                        let _ = raw_tx.send(vec![BACKCHANNEL_MSG_ERROR].into()).await;
                                    }
                                }

                                // Data received from remote endpoint.
                                res = remote_rx.recv() => {
                                    let value = match res {
                                        Ok(Some(value)) => value,
                                        Ok(None) => break,
                                        Err(err) => Err(err),
                                    };
                                    if tx.send(value).await.is_err() {
                                        break;
                                    }
                                }
                            }
                        }
                    })
                    .map(|_| ())
                    .boxed()
                })?)
            }
            None => {
                // Channel is closed.
                None
            }
        };

        // Encode chmux port number in transport type and serialize it.
        let transported = TransportedSender::<T, Codec> { port, data: PhantomData, codec: PhantomData };
        transported.serialize(serializer)
    }
}

impl<'de, T, Codec, const BUFFER: usize> Deserialize<'de> for Sender<T, Codec, BUFFER>
where
    T: Serialize + DeserializeOwned + Send + 'static,
    Codec: CodecT,
{
    /// Deserializes this sender after it has been received over a chmux channel.
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        assert!(BUFFER > 0, "BUFFER must not be zero");

        // Get chmux port number from deserialized transport type.
        let TransportedSender { port, .. } = TransportedSender::<T, Codec>::deserialize(deserializer)?;

        match port {
            // Received channel is open.
            Some(port) => {
                // Create internal communication channels.
                let (tx, mut rx) = tokio::sync::mpsc::channel(BUFFER);
                let (closed_tx, closed_rx) = tokio::sync::watch::channel(false);
                let (remote_send_err_tx, remote_send_err_rx) = tokio::sync::watch::channel(None);

                // Accept chmux port request.
                let err_tx = tx.clone();
                PortDeserializer::accept(port, |local_port, request, allocator| {
                    tokio::spawn(async move {
                        // Accept chmux connection request.
                        let (raw_tx, mut raw_rx) = match request.accept_from(local_port).await {
                            Ok(tx_rx) => tx_rx,
                            Err(err) => {
                                let _ = err_tx.send(Err(remote::ReceiveError::Listen(err))).await;
                                return;
                            }
                        };

                        // Encode data using remote sender for sending.
                        let mut remote_tx = remote::Sender::<Result<T, remote::ReceiveError>, Codec>::new(
                            Obtainer::ready(Ok(raw_tx)),
                            allocator,
                        );

                        // Process events.
                        let mut backchannel_active = true;
                        loop {
                            tokio::select! {
                                biased;

                                // Backchannel message from remote endpoint.
                                backchannel_msg = raw_rx.recv(), if backchannel_active => {
                                    match backchannel_msg {
                                        Ok(Some(mut msg)) if msg.remaining() >= 1 => {
                                            match msg.get_u8() {
                                                BACKCHANNEL_MSG_CLOSE => {
                                                    let _ = closed_tx.send(true);
                                                }
                                                BACKCHANNEL_MSG_ERROR => {
                                                    let _ = remote_send_err_tx.send(Some(SendErrorKind::Forward));
                                                }
                                                _ => (),
                                            }
                                        },
                                        _ => backchannel_active = false,
                                    }
                                }

                                // Data to send to remote endpoint.
                                value_opt = rx.recv() => {
                                    match value_opt {
                                        Some(value) => {
                                            if let Err(err) = remote_tx.send(value).await {
                                                let _ = remote_send_err_tx.send(Some(err.kind));
                                            }
                                        }
                                        None => break,
                                    }
                                }
                            }
                        }
                    })
                    .map(|_| ())
                    .boxed()
                })?;

                Ok(Self::new(tx, closed_rx, remote_send_err_rx))
            }

            // Received closed channel.
            None => Ok(Self::new_closed()),
        }
    }
}
