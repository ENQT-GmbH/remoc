use bytes::Buf;
use futures::FutureExt;
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use std::{marker::PhantomData, sync::Mutex};

use crate::{
    codec::CodecT,
    mpsc::{BACKCHANNEL_MSG_CLOSE, BACKCHANNEL_MSG_ERROR},
    remote::{self, Obtainer, PortDeserializer, PortSerializer, SendErrorKind},
};

pub enum ReceiveError {
    Remote(remote::ReceiveError),
}

/// Receive values from the associated [Sender], which may be located on a remote endpoint.
///
/// Instances are created by the [channel] function.
pub struct Receiver<T, Codec, const BUFFER: usize> {
    inner: Option<ReceiverInner<T, Codec, BUFFER>>,
    #[allow(clippy::type_complexity)]
    successor_tx: Mutex<Option<tokio::sync::oneshot::Sender<ReceiverInner<T, Codec, BUFFER>>>>,
}

pub(crate) struct ReceiverInner<T, Codec, const BUFFER: usize> {
    rx: tokio::sync::mpsc::Receiver<Result<T, remote::ReceiveError>>,
    closed_tx: tokio::sync::watch::Sender<bool>,
    remote_send_err_tx: tokio::sync::watch::Sender<Option<remote::SendErrorKind>>,
    _codec: PhantomData<Codec>,
}

#[derive(Serialize, Deserialize)]
pub struct TransportedReceiver<T, Codec> {
    /// chmux port number.
    port: u32,
    /// Data type.
    data: PhantomData<T>,
    /// Data codec.
    codec: PhantomData<Codec>,
}

impl<T, Codec, const BUFFER: usize> Receiver<T, Codec, BUFFER> {
    pub(crate) fn new(
        rx: tokio::sync::mpsc::Receiver<Result<T, remote::ReceiveError>>,
        closed_tx: tokio::sync::watch::Sender<bool>,
        remote_send_err_tx: tokio::sync::watch::Sender<Option<remote::SendErrorKind>>,
    ) -> Self {
        Self {
            inner: Some(ReceiverInner { rx, closed_tx, remote_send_err_tx, _codec: PhantomData }),
            successor_tx: Mutex::new(None),
        }
    }

    /// Receives the next value for this receiver.
    pub async fn recv(&mut self) -> Result<Option<T>, ReceiveError> {
        match self.inner.as_mut().unwrap().rx.recv().await {
            Some(Ok(value_opt)) => Ok(Some(value_opt)),
            Some(Err(err)) => Err(ReceiveError::Remote(err)),
            None => Ok(None),
        }
    }

    /// Closes the receiving half of a channel without dropping it.
    pub fn close(&mut self) {
        let _ = self.inner.as_mut().unwrap().closed_tx.send(true);
    }
}

impl<T, Codec, const BUFFER: usize> Drop for Receiver<T, Codec, BUFFER> {
    fn drop(&mut self) {
        let mut successor_tx = self.successor_tx.lock().unwrap();
        if let Some(successor_tx) = successor_tx.take() {
            let _ = successor_tx.send(self.inner.take().unwrap());
        }
    }
}

impl<T, Codec, const BUFFER: usize> Serialize for Receiver<T, Codec, BUFFER>
where
    T: Serialize + DeserializeOwned + Send + 'static,
    Codec: CodecT,
{
    /// Serializes this receiver for sending over a chmux channel.
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        // Register successor of this receiver.
        let (successor_tx, successor_rx) = tokio::sync::oneshot::channel();
        *self.successor_tx.lock().unwrap() = Some(successor_tx);

        let port = PortSerializer::connect(|connect, allocator| {
            tokio::spawn(async move {
                // Receiver has been dropped after sending, so we receive its channels.
                let ReceiverInner { mut rx, closed_tx, remote_send_err_tx, _codec } = match successor_rx.await {
                    Ok(inner) => inner,
                    Err(_) => return,
                };

                // Establish chmux channel.
                let (raw_tx, mut raw_rx) = match connect.await {
                    Ok(tx_rx) => tx_rx,
                    Err(err) => {
                        let _ = remote_send_err_tx.send(Some(remote::SendErrorKind::Connect(err)));
                        return;
                    }
                };

                // Encode data using remote sender.
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
                        res_opt = rx.recv() => {
                            let res = match res_opt {
                                Some(res) => res,
                                None => break,
                            };

                            if let Err(err) = remote_tx.send(res).await {
                                let _ = remote_send_err_tx.send(Some(err.kind));
                            }
                        }
                    }
                }
            })
            .map(|_| ())
            .boxed()
        })?;

        // Encode chmux port number in transport type and serialize it.
        let transported = TransportedReceiver::<T, Codec> { port, data: PhantomData, codec: PhantomData };
        transported.serialize(serializer)
    }
}

impl<'de, T, Codec, const BUFFER: usize> Deserialize<'de> for Receiver<T, Codec, BUFFER>
where
    T: Serialize + DeserializeOwned + Send + 'static,
    Codec: CodecT,
{
    /// Deserializes the receiver after it has been received over a chmux channel.
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        assert!(BUFFER > 0, "BUFFER must not be zero");

        // Get chmux port number from deserialized transport type.
        let TransportedReceiver { port, .. } = TransportedReceiver::<T, Codec>::deserialize(deserializer)?;

        // Create channels.
        let (tx, rx) = tokio::sync::mpsc::channel(BUFFER);
        let (closed_tx, mut closed_rx) = tokio::sync::watch::channel(false);
        let (remote_send_err_tx, mut remote_send_err_rx) = tokio::sync::watch::channel(None);

        PortDeserializer::accept(port, |local_port, request, allocator| {
            tokio::spawn(async move {
                // Accept chmux connection request.
                let (mut raw_tx, raw_rx) = match request.accept_from(local_port).await {
                    Ok(tx_rx) => tx_rx,
                    Err(err) => {
                        let _ = tx.send(Err(remote::ReceiveError::Listen(err))).await;
                        return;
                    }
                };

                // Decode received data using remote receiver.
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
        })?;

        Ok(Self::new(rx, closed_tx, remote_send_err_tx))
    }
}
