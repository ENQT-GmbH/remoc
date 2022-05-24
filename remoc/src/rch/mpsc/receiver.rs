use bytes::Buf;
use futures::{ready, FutureExt, Stream};
use serde::{Deserialize, Serialize};
use std::{
    convert::TryFrom,
    error::Error,
    fmt,
    marker::PhantomData,
    pin::Pin,
    sync::Mutex,
    task::{Context, Poll},
};

use super::{
    super::{
        base::{self, PortDeserializer, PortSerializer},
        ClosedReason, RemoteSendError, BACKCHANNEL_MSG_CLOSE, BACKCHANNEL_MSG_ERROR, DEFAULT_BUFFER,
    },
    recv_impl, send_impl, Distributor,
};
use crate::{chmux, codec, RemoteSend};

/// An error occurred during receiving over an mpsc channel.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum RecvError {
    /// Receiving from a remote endpoint failed.
    RemoteReceive(base::RecvError),
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

impl TryFrom<TryRecvError> for RecvError {
    type Error = TryRecvError;

    fn try_from(err: TryRecvError) -> Result<Self, Self::Error> {
        match err {
            TryRecvError::Closed => Err(TryRecvError::Closed),
            TryRecvError::Empty => Err(TryRecvError::Empty),
            TryRecvError::RemoteReceive(err) => Ok(Self::RemoteReceive(err)),
            TryRecvError::RemoteConnect(err) => Ok(Self::RemoteConnect(err)),
            TryRecvError::RemoteListen(err) => Ok(Self::RemoteListen(err)),
        }
    }
}

impl Error for RecvError {}

impl RecvError {
    /// Returns whether the error is final, i.e. no further receive operation can succeed.
    pub fn is_final(&self) -> bool {
        match self {
            Self::RemoteReceive(err) => err.is_final(),
            Self::RemoteConnect(_) | Self::RemoteListen(_) => true,
        }
    }
}

/// An error occurred during trying to receive over an mpsc channel.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum TryRecvError {
    /// All channel senders have been dropped.
    Closed,
    /// Currently no value is ready to receive, but values may still arrive
    /// in the future.
    Empty,
    /// Receiving from a remote endpoint failed.
    RemoteReceive(base::RecvError),
    /// Connecting a sent channel failed.
    RemoteConnect(chmux::ConnectError),
    /// Listening for a connection from a received channel failed.
    RemoteListen(chmux::ListenerError),
}

impl fmt::Display for TryRecvError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Self::Closed => write!(f, "channel is closed"),
            Self::Empty => write!(f, "channel is empty"),
            Self::RemoteReceive(err) => write!(f, "receive error: {}", err),
            Self::RemoteConnect(err) => write!(f, "connect error: {}", err),
            Self::RemoteListen(err) => write!(f, "listen error: {}", err),
        }
    }
}

impl From<RecvError> for TryRecvError {
    fn from(err: RecvError) -> Self {
        match err {
            RecvError::RemoteReceive(err) => Self::RemoteReceive(err),
            RecvError::RemoteConnect(err) => Self::RemoteConnect(err),
            RecvError::RemoteListen(err) => Self::RemoteListen(err),
        }
    }
}

impl Error for TryRecvError {}

impl TryRecvError {
    /// Returns whether the error is final, i.e. no further receive operation can succeed.
    pub fn is_final(&self) -> bool {
        match self {
            Self::Empty => false,
            Self::RemoteReceive(err) => err.is_final(),
            Self::Closed | Self::RemoteConnect(_) | Self::RemoteListen(_) => true,
        }
    }
}

/// Receive values from the associated [Sender](super::Sender),
/// which may be located on a remote endpoint.
///
/// Instances are created by the [channel](super::channel) function.
pub struct Receiver<T, Codec = codec::Default, const BUFFER: usize = DEFAULT_BUFFER> {
    inner: Option<ReceiverInner<T>>,
    #[allow(clippy::type_complexity)]
    successor_tx: Mutex<Option<tokio::sync::oneshot::Sender<ReceiverInner<T>>>>,
    final_err: Option<RecvError>,
    _codec: PhantomData<Codec>,
}

impl<T, Codec, const BUFFER: usize> fmt::Debug for Receiver<T, Codec, BUFFER> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("Receiver").finish()
    }
}

pub(crate) struct ReceiverInner<T> {
    rx: tokio::sync::mpsc::Receiver<Result<T, RecvError>>,
    closed_tx: tokio::sync::watch::Sender<Option<ClosedReason>>,
    remote_send_err_tx: tokio::sync::watch::Sender<Option<RemoteSendError>>,
    closed: bool,
}

/// Mpsc receiver in transport.
#[derive(Serialize, Deserialize)]
pub(crate) struct TransportedReceiver<T, Codec> {
    /// chmux port number.
    port: u32,
    /// Data type.
    data: PhantomData<T>,
    /// Data codec.
    codec: PhantomData<Codec>,
    /// Receiver has been closed.
    #[serde(default)]
    closed: bool,
}

impl<T, Codec, const BUFFER: usize> Receiver<T, Codec, BUFFER> {
    pub(crate) fn new(
        rx: tokio::sync::mpsc::Receiver<Result<T, RecvError>>,
        closed_tx: tokio::sync::watch::Sender<Option<ClosedReason>>, closed: bool,
        remote_send_err_tx: tokio::sync::watch::Sender<Option<RemoteSendError>>,
    ) -> Self {
        Self {
            inner: Some(ReceiverInner { rx, closed_tx, remote_send_err_tx, closed }),
            successor_tx: Mutex::new(None),
            final_err: None,
            _codec: PhantomData,
        }
    }

    /// Receives the next value for this receiver.
    ///
    /// This function returns `Ok(None)` when all channel senders have been dropped.
    ///
    /// When a receive error occurs due to a connection failure and other senders are still
    /// present, it is held back and returned after all other senders have been dropped or failed.
    /// Use [error](Self::error) to check if such an error is present.
    #[inline]
    pub async fn recv(&mut self) -> Result<Option<T>, RecvError> {
        loop {
            match self.inner.as_mut().unwrap().rx.recv().await {
                Some(Ok(value_opt)) => return Ok(Some(value_opt)),
                Some(Err(err)) => {
                    if err.is_final() {
                        if self.final_err.is_none() {
                            self.final_err = Some(err);
                        }
                        continue;
                    } else {
                        return Err(err);
                    }
                }
                None => match self.take_error() {
                    Some(err) => return Err(err),
                    None => return Ok(None),
                },
            }
        }
    }

    /// Polls to receive the next message on this channel.
    ///
    /// This function returns `Poll::Ready(Ok(None))` when all channel senders have been dropped.
    ///
    /// When a receive error occurs due to a connection failure and other senders are still
    /// present, it is held back and returned after all other senders have been dropped or failed.
    /// Use [error](Self::error) to check if such an error is present.
    #[inline]
    pub fn poll_recv(&mut self, cx: &mut Context) -> Poll<Result<Option<T>, RecvError>> {
        loop {
            match ready!(self.inner.as_mut().unwrap().rx.poll_recv(cx)) {
                Some(Ok(value_opt)) => return Poll::Ready(Ok(Some(value_opt))),
                Some(Err(err)) => {
                    if err.is_final() {
                        if self.final_err.is_none() {
                            self.final_err = Some(err);
                        }
                        continue;
                    } else {
                        return Poll::Ready(Err(err));
                    }
                }
                None => match self.take_error() {
                    Some(err) => return Poll::Ready(Err(err)),
                    None => return Poll::Ready(Ok(None)),
                },
            }
        }
    }

    /// Tries to receive the next message on this channel, if one is immediately available.
    ///
    /// This function returns `Err(RecvError::Closed)` when all channel senders have been dropped
    /// and `Err(RecvError::Empty)` if no value to receive is currently available.
    ///
    /// When a receive error occurs due to a connection failure and other senders are still
    /// present, it is held back and returned after all other senders have been dropped or failed.
    /// Use [error](Self::error) to check if such an error is present.
    #[inline]
    pub fn try_recv(&mut self) -> Result<T, TryRecvError> {
        loop {
            match self.inner.as_mut().unwrap().rx.try_recv() {
                Ok(Ok(value_opt)) => return Ok(value_opt),
                Ok(Err(err)) => {
                    if err.is_final() {
                        if self.final_err.is_none() {
                            self.final_err = Some(err);
                        }
                        continue;
                    } else {
                        return Err(err.into());
                    }
                }
                Err(tokio::sync::mpsc::error::TryRecvError::Disconnected) => match self.take_error() {
                    Some(err) => return Err(err.into()),
                    None => return Err(TryRecvError::Closed),
                },
                Err(tokio::sync::mpsc::error::TryRecvError::Empty) => return Err(TryRecvError::Empty),
            }
        }
    }

    /// Blocking receive to call outside of asynchronous contexts.
    ///
    /// This function returns `Ok(None)` when the channel sender has been dropped.
    ///
    /// # Panics
    /// This function panics if called within an asynchronous execution context.
    #[inline]
    pub fn blocking_recv(&mut self) -> Result<Option<T>, RecvError> {
        let rt = tokio::runtime::Builder::new_current_thread().build().unwrap();
        rt.block_on(self.recv())
    }

    /// Closes the receiving half of a channel without dropping it.
    ///
    /// This allows to process outstanding values while stopping the sender from
    /// sending new values.
    #[inline]
    pub fn close(&mut self) {
        let mut inner = self.inner.as_mut().unwrap();
        let _ = inner.closed_tx.send(Some(ClosedReason::Closed));
        inner.closed = true;
    }

    /// Returns the first error that occurred during receiving due to a connection failure,
    /// but is being held back because other senders are still connected to this receiver.
    ///
    /// Use [take_error](Self::take_error) to clear it.
    pub fn error(&self) -> &Option<RecvError> {
        &self.final_err
    }

    /// Returns the held back error and clears it.
    ///
    /// See [recv](Self::recv) and [error](Self::error) for details.
    pub fn take_error(&mut self) -> Option<RecvError> {
        self.final_err.take()
    }

    /// Sets the codec that will be used when sending this receiver to a remote endpoint.
    pub fn set_codec<NewCodec>(mut self) -> Receiver<T, NewCodec, BUFFER> {
        Receiver {
            inner: self.inner.take(),
            successor_tx: Mutex::new(None),
            final_err: self.final_err.clone(),
            _codec: PhantomData,
        }
    }

    /// Sets the buffer size that will be used when sending this receiver to a remote endpoint.
    pub fn set_buffer<const NEW_BUFFER: usize>(mut self) -> Receiver<T, Codec, NEW_BUFFER> {
        assert!(NEW_BUFFER > 0, "buffer size must not be zero");
        Receiver {
            inner: self.inner.take(),
            successor_tx: Mutex::new(None),
            final_err: self.final_err.clone(),
            _codec: PhantomData,
        }
    }
}

impl<T, Codec, const BUFFER: usize> Receiver<T, Codec, BUFFER>
where
    T: RemoteSend + Clone,
    Codec: codec::Codec,
{
    /// Distribute received items over multiple receivers.
    ///
    /// Each value is received by one of the receivers.
    ///
    /// If `wait_on_empty` is true, the distributor waits if all subscribers are closed.
    /// Otherwise it terminates.
    pub fn distribute(self, wait_on_empty: bool) -> Distributor<T, Codec, BUFFER> {
        Distributor::new(self, wait_on_empty)
    }
}

impl<T, Codec, const BUFFER: usize> Drop for Receiver<T, Codec, BUFFER> {
    fn drop(&mut self) {
        if let Some(inner) = self.inner.take() {
            let mut successor_tx = self.successor_tx.lock().unwrap();
            if let Some(successor_tx) = successor_tx.take() {
                let _ = successor_tx.send(inner);
            } else if !inner.closed {
                let _ = inner.closed_tx.send(Some(ClosedReason::Dropped));
            }
        }
    }
}

impl<T, Codec, const BUFFER: usize> Serialize for Receiver<T, Codec, BUFFER>
where
    T: RemoteSend,
    Codec: codec::Codec,
{
    /// Serializes this receiver for sending over a chmux channel.
    #[inline]
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
                let ReceiverInner { mut rx, closed_tx, remote_send_err_tx, closed: _ } = match successor_rx.await
                {
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

                send_impl!(T, rx, raw_tx, raw_rx, remote_send_err_tx, closed_tx);
            }
            .boxed()
        })?;

        // Encode chmux port number in transport type and serialize it.
        let transported = TransportedReceiver::<T, Codec> {
            port,
            data: PhantomData,
            codec: PhantomData,
            closed: self.inner.as_ref().unwrap().closed,
        };
        transported.serialize(serializer)
    }
}

impl<'de, T, Codec, const BUFFER: usize> Deserialize<'de> for Receiver<T, Codec, BUFFER>
where
    T: RemoteSend,
    Codec: codec::Codec,
{
    /// Deserializes the receiver after it has been received over a chmux channel.
    #[inline]
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        assert!(BUFFER > 0, "BUFFER must not be zero");

        // Get chmux port number from deserialized transport type.
        let TransportedReceiver { port, closed, .. } =
            TransportedReceiver::<T, Codec>::deserialize(deserializer)?;

        // Create channels.
        let (tx, rx) = tokio::sync::mpsc::channel(BUFFER);
        let (closed_tx, mut closed_rx) = tokio::sync::watch::channel(None);
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

                recv_impl!(T, tx, raw_tx, raw_rx, remote_send_err_rx, closed_rx);
            }
            .boxed()
        })?;

        Ok(Self::new(rx, closed_tx, closed, remote_send_err_tx))
    }
}

impl<T, Codec, const BUFFER: usize> Stream for Receiver<T, Codec, BUFFER> {
    type Item = Result<T, RecvError>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Option<Self::Item>> {
        let res = ready!(Pin::into_inner(self).poll_recv(cx));
        Poll::Ready(res.transpose())
    }
}

impl<T, Codec, const BUFFER: usize> Unpin for Receiver<T, Codec, BUFFER> {}
