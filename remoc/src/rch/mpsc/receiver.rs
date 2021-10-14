use bytes::Buf;
use futures::{ready, FutureExt};
use serde::{Deserialize, Serialize};
use std::{
    error::Error,
    fmt,
    marker::PhantomData,
    sync::Mutex,
    task::{Context, Poll},
};

use super::{
    super::{
        buffer,
        remote::{self, PortDeserializer, PortSerializer},
        RemoteSendError, BACKCHANNEL_MSG_CLOSE, BACKCHANNEL_MSG_ERROR,
    },
    Distributor,
};
use crate::{chmux, codec, RemoteSend};

/// An error occured during receiving over an mpsc channel.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum RecvError {
    /// Receiving from a remote endpoint failed.
    RemoteReceive(remote::RecvError),
    /// Connecting a sent channel failed.
    RemoteConnect(chmux::ConnectError),
    /// Listening for a connection from a received channel failed.
    RemoteListen(chmux::ListenerError),
}

impl fmt::Display for RecvError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Self::RemoteReceive(err) => write!(f, "receive error: {}", err),
            Self::RemoteConnect(err) => write!(f, "connect error: {}", err),
            Self::RemoteListen(err) => write!(f, "listen error: {}", err),
        }
    }
}

impl Error for RecvError {}

/// Receive values from the associated [Sender](super::Sender),
/// which may be located on a remote endpoint.
///
/// Instances are created by the [channel](super::channel) function.
pub struct Receiver<T, Codec = codec::Default, Buffer = buffer::Default> {
    inner: Option<ReceiverInner<T>>,
    #[allow(clippy::type_complexity)]
    successor_tx: Mutex<Option<tokio::sync::oneshot::Sender<ReceiverInner<T>>>>,
    _codec: PhantomData<Codec>,
    _buffer: PhantomData<Buffer>,
}

impl<T, Codec, Buffer> fmt::Debug for Receiver<T, Codec, Buffer> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("Receiver").finish_non_exhaustive()
    }
}

pub(crate) struct ReceiverInner<T> {
    rx: tokio::sync::mpsc::Receiver<Result<T, RecvError>>,
    closed_tx: tokio::sync::watch::Sender<bool>,
    remote_send_err_tx: tokio::sync::watch::Sender<Option<RemoteSendError>>,
}

/// Mpsc receiver in transport.
#[derive(Serialize, Deserialize)]
pub struct TransportedReceiver<T, Codec> {
    /// chmux port number.
    port: u32,
    /// Data type.
    data: PhantomData<T>,
    /// Data codec.
    codec: PhantomData<Codec>,
}

impl<T, Codec, Buffer> Receiver<T, Codec, Buffer> {
    pub(crate) fn new(
        rx: tokio::sync::mpsc::Receiver<Result<T, RecvError>>, closed_tx: tokio::sync::watch::Sender<bool>,
        remote_send_err_tx: tokio::sync::watch::Sender<Option<RemoteSendError>>,
    ) -> Self {
        Self {
            inner: Some(ReceiverInner { rx, closed_tx, remote_send_err_tx }),
            successor_tx: Mutex::new(None),
            _codec: PhantomData,
            _buffer: PhantomData,
        }
    }

    /// Receives the next value for this receiver.
    pub async fn recv(&mut self) -> Result<Option<T>, RecvError> {
        match self.inner.as_mut().unwrap().rx.recv().await {
            Some(Ok(value_opt)) => Ok(Some(value_opt)),
            Some(Err(err)) => Err(err),
            None => Ok(None),
        }
    }

    /// Polls to receive the next message on this channel.
    pub fn poll_recv(&mut self, cx: &mut Context<'_>) -> Poll<Result<Option<T>, RecvError>> {
        match ready!(self.inner.as_mut().unwrap().rx.poll_recv(cx)) {
            Some(Ok(value_opt)) => Poll::Ready(Ok(Some(value_opt))),
            Some(Err(err)) => Poll::Ready(Err(err)),
            None => Poll::Ready(Ok(None)),
        }
    }

    /// Closes the receiving half of a channel without dropping it.
    pub fn close(&mut self) {
        let _ = self.inner.as_mut().unwrap().closed_tx.send(true);
    }

    /// Sets the codec that will be used when sending this receiver to a remote endpoint.
    pub fn set_codec<NewCodec>(mut self) -> Receiver<T, NewCodec, Buffer> {
        Receiver {
            inner: self.inner.take(),
            successor_tx: Mutex::new(None),
            _codec: PhantomData,
            _buffer: PhantomData,
        }
    }

    /// Sets the buffer size that will be used when sending this receiver to a remote endpoint.
    pub fn set_buffer<NewBuffer>(mut self) -> Receiver<T, Codec, NewBuffer>
    where
        NewBuffer: buffer::Size,
    {
        assert!(NewBuffer::size() > 0, "buffer size must not be zero");
        Receiver {
            inner: self.inner.take(),
            successor_tx: Mutex::new(None),
            _codec: PhantomData,
            _buffer: PhantomData,
        }
    }
}

impl<T, Codec, Buffer> Receiver<T, Codec, Buffer>
where
    T: RemoteSend + Clone,
    Codec: codec::Codec,
    Buffer: buffer::Size,
{
    /// Distribute received items over multiple receivers.
    ///
    /// Each value is received by one of the receivers.
    ///
    /// If `wait_on_empty` is true, the distributor waits if all subscribers are closed.
    /// Otherwise it terminates.
    pub fn distribute(self, wait_on_empty: bool) -> Distributor<T, Codec, Buffer> {
        Distributor::new(self, wait_on_empty)
    }
}

impl<T, Codec, Buffer> Drop for Receiver<T, Codec, Buffer> {
    fn drop(&mut self) {
        let mut successor_tx = self.successor_tx.lock().unwrap();
        if let Some(successor_tx) = successor_tx.take() {
            let _ = successor_tx.send(self.inner.take().unwrap());
        }
    }
}

impl<T, Codec, Buffer> Serialize for Receiver<T, Codec, Buffer>
where
    T: RemoteSend,
    Codec: codec::Codec,
    Buffer: buffer::Size,
{
    /// Serializes this receiver for sending over a chmux channel.
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        // Register successor of this receiver.
        let (successor_tx, successor_rx) = tokio::sync::oneshot::channel();
        *self.successor_tx.lock().unwrap() = Some(successor_tx);

        let port = PortSerializer::connect(|connect| {
            async move {
                // Receiver has been dropped after sending, so we receive its channels.
                let ReceiverInner { mut rx, closed_tx, remote_send_err_tx } = match successor_rx.await {
                    Ok(inner) => inner,
                    Err(_) => return,
                };

                // Establish chmux channel.
                let (raw_tx, mut raw_rx) = match connect.await {
                    Ok(tx_rx) => tx_rx,
                    Err(err) => {
                        let _ = remote_send_err_tx.send(Some(RemoteSendError::Connect(err)));
                        return;
                    }
                };

                // Encode data using remote sender.
                let mut remote_tx = remote::Sender::<Result<T, RecvError>, Codec>::new(raw_tx);

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
                                            let _ = remote_send_err_tx.send(Some(RemoteSendError::Forward));
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
                                let _ = remote_send_err_tx.send(Some(RemoteSendError::Send(err.kind)));
                            }
                        }
                    }
                }
            }
            .boxed()
        })?;

        // Encode chmux port number in transport type and serialize it.
        let transported = TransportedReceiver::<T, Codec> { port, data: PhantomData, codec: PhantomData };
        transported.serialize(serializer)
    }
}

impl<'de, T, Codec, Buffer> Deserialize<'de> for Receiver<T, Codec, Buffer>
where
    T: RemoteSend,
    Codec: codec::Codec,
    Buffer: buffer::Size,
{
    /// Deserializes the receiver after it has been received over a chmux channel.
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        assert!(Buffer::size() > 0, "BUFFER must not be zero");

        // Get chmux port number from deserialized transport type.
        let TransportedReceiver { port, .. } = TransportedReceiver::<T, Codec>::deserialize(deserializer)?;

        // Create channels.
        let (tx, rx) = tokio::sync::mpsc::channel(Buffer::size());
        let (closed_tx, mut closed_rx) = tokio::sync::watch::channel(false);
        let (remote_send_err_tx, mut remote_send_err_rx) = tokio::sync::watch::channel(None);

        PortDeserializer::accept(port, |local_port, request| {
            async move {
                // Accept chmux connection request.
                let (mut raw_tx, raw_rx) = match request.accept_from(local_port).await {
                    Ok(tx_rx) => tx_rx,
                    Err(err) => {
                        let _ = tx.send(Err(RecvError::RemoteListen(err))).await;
                        return;
                    }
                };

                // Decode received data using remote receiver.
                let mut remote_rx = remote::Receiver::<Result<T, RecvError>, Codec>::new(raw_rx);

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
                                Err(err) => Err(RecvError::RemoteReceive(err)),
                            };
                            if tx.send(value).await.is_err() {
                                break;
                            }
                        }
                    }
                }
            }
            .boxed()
        })?;

        Ok(Self::new(rx, closed_tx, remote_send_err_tx))
    }
}
