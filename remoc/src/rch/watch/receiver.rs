use bytes::Buf;
use futures::{ready, FutureExt, Stream};
use serde::{Deserialize, Serialize};
use std::{
    error::Error,
    fmt,
    marker::PhantomData,
    pin::Pin,
    task::{Context, Poll},
};
use tokio_util::sync::ReusableBoxFuture;

use super::{
    super::{
        base::{self, PortDeserializer, PortSerializer},
        RemoteSendError, BACKCHANNEL_MSG_ERROR,
    },
    recv_impl, send_impl, Ref, ERROR_QUEUE,
};
use crate::{chmux, codec, RemoteSend};

/// An error occurred during receiving over a watch channel.
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

/// An error occurred during waiting for a change on a watch channel.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum ChangedError {
    /// The sender has been dropped or the connection has been lost.
    Closed,
}

impl ChangedError {
    /// True, if remote endpoint has closed channel.
    #[deprecated = "a remoc::rch::watch::ChangedError is always due to closure"]
    pub fn is_closed(&self) -> bool {
        true
    }
}

impl fmt::Display for ChangedError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Self::Closed => write!(f, "closed"),
        }
    }
}

impl Error for ChangedError {}

/// Receive values from the associated [Sender](super::Sender),
/// which may be located on a remote endpoint.
///
/// Instances are created by the [channel](super::channel) function.
///
/// This can be converted into a [Stream](futures::Stream) of values by wrapping it into
/// a [ReceiverStream].
#[derive(Clone)]
pub struct Receiver<T, Codec = codec::Default> {
    rx: tokio::sync::watch::Receiver<Result<T, RecvError>>,
    remote_send_err_tx: tokio::sync::mpsc::Sender<RemoteSendError>,
    _codec: PhantomData<Codec>,
}

impl<T, Codec> fmt::Debug for Receiver<T, Codec> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("Receiver").finish()
    }
}

/// Watch receiver in transport.
#[derive(Serialize, Deserialize)]
pub(crate) struct TransportedReceiver<T, Codec> {
    /// chmux port number.
    port: u32,
    /// Current data value.
    data: Result<T, RecvError>,
    /// Data codec.
    codec: PhantomData<Codec>,
}

impl<T, Codec> Receiver<T, Codec> {
    pub(crate) fn new(
        rx: tokio::sync::watch::Receiver<Result<T, RecvError>>,
        remote_send_err_tx: tokio::sync::mpsc::Sender<RemoteSendError>,
    ) -> Self {
        Self { rx, remote_send_err_tx, _codec: PhantomData }
    }

    /// Returns a reference to the most recently received value.
    #[inline]
    pub fn borrow(&self) -> Result<Ref<'_, T>, RecvError> {
        let ref_res = self.rx.borrow();
        match &*ref_res {
            Ok(_) => Ok(Ref(ref_res)),
            Err(err) => Err(err.clone()),
        }
    }

    /// Returns a reference to the most recently received value and mark that value as seen.
    #[inline]
    pub fn borrow_and_update(&mut self) -> Result<Ref<'_, T>, RecvError> {
        let ref_res = self.rx.borrow_and_update();
        match &*ref_res {
            Ok(_) => Ok(Ref(ref_res)),
            Err(err) => Err(err.clone()),
        }
    }

    /// Wait for a change notification, then mark the newest value as seen.
    #[inline]
    pub async fn changed(&mut self) -> Result<(), ChangedError> {
        self.rx.changed().await.map_err(|_| ChangedError::Closed)
    }
}

impl<T, Codec> Drop for Receiver<T, Codec> {
    fn drop(&mut self) {
        // empty
    }
}

impl<T, Codec> Serialize for Receiver<T, Codec>
where
    T: RemoteSend + Sync + Clone,
    Codec: codec::Codec,
{
    /// Serializes this receiver for sending over a chmux channel.
    #[inline]
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        // Prepare channel for takeover.
        let mut rx = self.rx.clone();
        let remote_send_err_tx = self.remote_send_err_tx.clone();

        let port = PortSerializer::connect(|connect| {
            async move {
                // Establish chmux channel.
                let (raw_tx, mut raw_rx) = match connect.await {
                    Ok(tx_rx) => tx_rx,
                    Err(err) => {
                        let _ = remote_send_err_tx.try_send(RemoteSendError::Connect(err));
                        return;
                    }
                };

                send_impl!(T, rx, raw_tx, raw_rx, remote_send_err_tx);
            }
            .boxed()
        })?;

        // Encode chmux port number in transport type and serialize it.
        let data = self.rx.borrow().clone();
        let transported = TransportedReceiver::<T, Codec> { port, data, codec: PhantomData };
        transported.serialize(serializer)
    }
}

impl<'de, T, Codec> Deserialize<'de> for Receiver<T, Codec>
where
    T: RemoteSend + Sync,
    Codec: codec::Codec,
{
    /// Deserializes the receiver after it has been received over a chmux channel.
    #[inline]
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        // Get chmux port number from deserialized transport type.
        let TransportedReceiver { port, data, .. } = TransportedReceiver::<T, Codec>::deserialize(deserializer)?;

        // Create channels.
        let (tx, rx) = tokio::sync::watch::channel(data);
        let (remote_send_err_tx, mut remote_send_err_rx) = tokio::sync::mpsc::channel(ERROR_QUEUE);

        PortDeserializer::accept(port, |local_port, request| {
            async move {
                // Accept chmux connection request.
                let (mut raw_tx, raw_rx) = match request.accept_from(local_port).await {
                    Ok(tx_rx) => tx_rx,
                    Err(err) => {
                        let _ = tx.send(Err(RecvError::RemoteListen(err)));
                        return;
                    }
                };

                let mut current_err: Option<RemoteSendError> = None;
                recv_impl!(T, tx, raw_tx, raw_rx, remote_send_err_rx, current_err);
            }
            .boxed()
        })?;

        Ok(Self::new(rx, remote_send_err_tx))
    }
}

/// A wrapper around a watch [Receiver] that implements [Stream](futures::Stream).
///
/// This stream will always start by yielding the current value when it is polled,
/// regardless of whether it was the initial value or sent afterwards.
///
/// Note that intermediate values may be missed due to the nature of watch channels.
pub struct ReceiverStream<T, Codec = codec::Default> {
    inner: ReusableBoxFuture<'static, (Result<(), ChangedError>, Receiver<T, Codec>)>,
}

impl<T, Codec> fmt::Debug for ReceiverStream<T, Codec> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("ReceiverStream").finish()
    }
}

impl<T, Codec> ReceiverStream<T, Codec>
where
    T: RemoteSend + Sync,
    Codec: Send + 'static,
{
    /// Creates a new `ReceiverStream`.
    pub fn new(rx: Receiver<T, Codec>) -> Self {
        Self { inner: ReusableBoxFuture::new(async move { (Ok(()), rx) }) }
    }

    async fn make_future(mut rx: Receiver<T, Codec>) -> (Result<(), ChangedError>, Receiver<T, Codec>) {
        let result = rx.changed().await;
        (result, rx)
    }
}

impl<T, Codec> Stream for ReceiverStream<T, Codec>
where
    T: Clone + RemoteSend + Sync,
    Codec: Send + 'static,
{
    type Item = Result<T, RecvError>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context) -> Poll<Option<Self::Item>> {
        let (result, mut rx) = ready!(self.inner.poll(cx));
        match result {
            Ok(()) => {
                let received = rx.borrow_and_update().map(|v| v.clone());
                self.inner.set(Self::make_future(rx));
                Poll::Ready(Some(received))
            }
            Err(_) => {
                self.inner.set(Self::make_future(rx));
                Poll::Ready(None)
            }
        }
    }
}

impl<T, Codec> Unpin for ReceiverStream<T, Codec> {}

impl<T, Codec> From<Receiver<T, Codec>> for ReceiverStream<T, Codec>
where
    T: RemoteSend + Sync,
    Codec: Send + 'static,
{
    fn from(recv: Receiver<T, Codec>) -> Self {
        Self::new(recv)
    }
}
