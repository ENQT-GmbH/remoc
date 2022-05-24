use futures::{ready, task::noop_waker, Stream};
use serde::{Deserialize, Serialize};
use std::{
    convert::TryFrom,
    error::Error,
    fmt,
    pin::Pin,
    task::{Context, Poll},
};
use tokio_util::sync::ReusableBoxFuture;

use super::{
    super::{base, mpsc, DEFAULT_BUFFER},
    BroadcastMsg,
};
use crate::{chmux, codec, RemoteSend};

/// An error occurred during receiving over a broadcast channel.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum RecvError {
    /// There are no more active senders implying no further messages will ever be sent.
    Closed,
    /// The receiver lagged too far behind.
    ///
    /// Attempting to receive again will return the oldest message still retained by the channel.
    Lagged,
    /// Receiving from a remote endpoint failed.
    RemoteReceive(base::RecvError),
    /// Connecting a sent channel failed.
    RemoteConnect(chmux::ConnectError),
    /// Listening for a connection from a received channel failed.
    RemoteListen(chmux::ListenerError),
}

impl RecvError {
    /// True, if all senders have been dropped.
    pub fn is_closed(&self) -> bool {
        matches!(self, Self::Closed)
    }

    /// True, if the receiver has lagged behind and messages have been lost.
    pub fn is_lagged(&self) -> bool {
        matches!(self, Self::Lagged)
    }

    /// Returns whether the error is final, i.e. no further receive operation can succeed.
    pub fn is_final(&self) -> bool {
        match self {
            Self::RemoteReceive(err) => err.is_final(),
            Self::Closed | Self::RemoteConnect(_) | Self::RemoteListen(_) => true,
            Self::Lagged => false,
        }
    }
}

impl fmt::Display for RecvError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Self::Closed => write!(f, "channel closed"),
            Self::Lagged => write!(f, "receiver lagged behind"),
            Self::RemoteReceive(err) => write!(f, "receive error: {}", err),
            Self::RemoteConnect(err) => write!(f, "connect error: {}", err),
            Self::RemoteListen(err) => write!(f, "listen error: {}", err),
        }
    }
}

impl From<mpsc::RecvError> for RecvError {
    fn from(err: mpsc::RecvError) -> Self {
        match err {
            mpsc::RecvError::RemoteReceive(err) => Self::RemoteReceive(err),
            mpsc::RecvError::RemoteConnect(err) => Self::RemoteConnect(err),
            mpsc::RecvError::RemoteListen(err) => Self::RemoteListen(err),
        }
    }
}

impl TryFrom<TryRecvError> for RecvError {
    type Error = TryRecvError;

    fn try_from(err: TryRecvError) -> Result<Self, Self::Error> {
        match err {
            TryRecvError::Empty => Err(TryRecvError::Empty),
            TryRecvError::Closed => Ok(Self::Closed),
            TryRecvError::Lagged => Ok(Self::Lagged),
            TryRecvError::RemoteReceive(err) => Ok(Self::RemoteReceive(err)),
            TryRecvError::RemoteConnect(err) => Ok(Self::RemoteConnect(err)),
            TryRecvError::RemoteListen(err) => Ok(Self::RemoteListen(err)),
        }
    }
}

impl Error for RecvError {}

/// An error occurred during trying to receive over a broadcast channel.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum TryRecvError {
    /// The channel is currently empty. There are still active sender, so data may yet become available.
    Empty,
    /// There are no more active senders implying no further messages will ever be sent.
    Closed,
    /// The receiver lagged too far behind.
    ///
    /// Attempting to receive again will return the oldest message still retained by the channel.
    Lagged,
    /// Receiving from a remote endpoint failed.
    RemoteReceive(base::RecvError),
    /// Connecting a sent channel failed.
    RemoteConnect(chmux::ConnectError),
    /// Listening for a connection from a received channel failed.
    RemoteListen(chmux::ListenerError),
}

impl TryRecvError {
    /// True, if no value is currently present.
    pub fn is_empty(&self) -> bool {
        matches!(self, Self::Empty)
    }

    /// True, if all senders have been dropped.
    pub fn is_closed(&self) -> bool {
        matches!(self, Self::Closed)
    }

    /// True, if the receiver has lagged behind and messages have been lost.
    pub fn is_lagged(&self) -> bool {
        matches!(self, Self::Lagged)
    }

    /// Returns whether the error is final, i.e. no further receive operation can succeed.
    pub fn is_final(&self) -> bool {
        match self {
            Self::RemoteReceive(err) => err.is_final(),
            Self::Closed | Self::RemoteConnect(_) | Self::RemoteListen(_) => true,
            Self::Empty | Self::Lagged => false,
        }
    }
}

impl fmt::Display for TryRecvError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Self::Empty => write!(f, "channel empty"),
            Self::Closed => write!(f, "channel closed"),
            Self::Lagged => write!(f, "receiver lagged behind"),
            Self::RemoteReceive(err) => write!(f, "receive error: {}", err),
            Self::RemoteConnect(err) => write!(f, "connect error: {}", err),
            Self::RemoteListen(err) => write!(f, "listen error: {}", err),
        }
    }
}

impl From<mpsc::RecvError> for TryRecvError {
    fn from(err: mpsc::RecvError) -> Self {
        match err {
            mpsc::RecvError::RemoteReceive(err) => Self::RemoteReceive(err),
            mpsc::RecvError::RemoteConnect(err) => Self::RemoteConnect(err),
            mpsc::RecvError::RemoteListen(err) => Self::RemoteListen(err),
        }
    }
}

impl From<RecvError> for TryRecvError {
    fn from(err: RecvError) -> Self {
        match err {
            RecvError::Closed => Self::Closed,
            RecvError::Lagged => Self::Lagged,
            RecvError::RemoteReceive(err) => Self::RemoteReceive(err),
            RecvError::RemoteConnect(err) => Self::RemoteConnect(err),
            RecvError::RemoteListen(err) => Self::RemoteListen(err),
        }
    }
}

impl Error for TryRecvError {}

/// Receiving-half of the broadcast channel.
///
/// Can be sent over a remote channel.
///
/// This can be converted into a [Stream](futures::Stream) of values by wrapping it into
/// a [ReceiverStream].
#[derive(Serialize, Deserialize)]
#[serde(bound(serialize = "T: RemoteSend, Codec: codec::Codec"))]
#[serde(bound(deserialize = "T: RemoteSend, Codec: codec::Codec"))]
pub struct Receiver<T, Codec = codec::Default, const BUFFER: usize = { DEFAULT_BUFFER }> {
    rx: mpsc::Receiver<BroadcastMsg<T>, Codec, BUFFER>,
}

impl<T, Codec, const BUFFER: usize> fmt::Debug for Receiver<T, Codec, BUFFER> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("Receiver").finish()
    }
}

impl<T, Codec, const BUFFER: usize> Receiver<T, Codec, BUFFER>
where
    T: RemoteSend,
    Codec: codec::Codec,
{
    pub(crate) fn new(rx: mpsc::Receiver<BroadcastMsg<T>, Codec, BUFFER>) -> Self {
        Self { rx }
    }

    /// Receives the next value for this receiver.
    #[inline]
    pub async fn recv(&mut self) -> Result<T, RecvError> {
        match self.rx.recv().await {
            Ok(Some(BroadcastMsg::Value(value))) => Ok(value),
            Ok(Some(BroadcastMsg::Lagged)) => Err(RecvError::Lagged),
            Ok(None) => Err(RecvError::Closed),
            Err(err) => Err(err.into()),
        }
    }

    /// Attempts to return a pending value on this receiver without awaiting.
    #[inline]
    pub fn try_recv(&mut self) -> Result<T, TryRecvError> {
        let waker = noop_waker();
        let mut cx = Context::from_waker(&waker);

        match self.rx.poll_recv(&mut cx) {
            Poll::Ready(Ok(Some(BroadcastMsg::Value(value)))) => Ok(value),
            Poll::Ready(Ok(Some(BroadcastMsg::Lagged))) => Err(TryRecvError::Lagged),
            Poll::Ready(Ok(None)) => Err(TryRecvError::Closed),
            Poll::Ready(Err(err)) => Err(err.into()),
            Poll::Pending => Err(TryRecvError::Empty),
        }
    }
}

impl<T, Codec, const BUFFER: usize> Drop for Receiver<T, Codec, BUFFER> {
    fn drop(&mut self) {
        // empty
    }
}

/// An error occurred during receiving over a broadcast channel receiver stream.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum StreamError {
    /// The receiver stream lagged too far behind.
    ///
    /// The next value will be the oldest message still retained by the channel.
    Lagged,
    /// Receiving from a remote endpoint failed.
    RemoteReceive(base::RecvError),
    /// Connecting a sent channel failed.
    RemoteConnect(chmux::ConnectError),
    /// Listening for a connection from a received channel failed.
    RemoteListen(chmux::ListenerError),
}

impl StreamError {
    /// True, if the receiver has lagged behind and messages have been lost.
    pub fn is_lagged(&self) -> bool {
        matches!(self, Self::Lagged)
    }

    /// Returns whether the error is final, i.e. no further receive operation can succeed.
    pub fn is_final(&self) -> bool {
        match self {
            Self::RemoteReceive(err) => err.is_final(),
            Self::RemoteConnect(_) | Self::RemoteListen(_) => true,
            Self::Lagged => false,
        }
    }
}

impl fmt::Display for StreamError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Self::Lagged => write!(f, "receiver lagged behind"),
            Self::RemoteReceive(err) => write!(f, "receive error: {}", err),
            Self::RemoteConnect(err) => write!(f, "connect error: {}", err),
            Self::RemoteListen(err) => write!(f, "listen error: {}", err),
        }
    }
}

impl TryFrom<RecvError> for StreamError {
    type Error = RecvError;
    fn try_from(err: RecvError) -> Result<Self, Self::Error> {
        match err {
            RecvError::Lagged => Ok(Self::Lagged),
            RecvError::RemoteReceive(err) => Ok(Self::RemoteReceive(err)),
            RecvError::RemoteConnect(err) => Ok(Self::RemoteConnect(err)),
            RecvError::RemoteListen(err) => Ok(Self::RemoteListen(err)),
            other => Err(other),
        }
    }
}

impl Error for StreamError {}

/// A wrapper around a broadcast [Receiver] that implements [Stream](futures::Stream).
pub struct ReceiverStream<T, Codec = codec::Default, const BUFFER: usize = DEFAULT_BUFFER> {
    #[allow(clippy::type_complexity)]
    inner: ReusableBoxFuture<'static, (Result<T, RecvError>, Receiver<T, Codec, BUFFER>)>,
}

impl<T, Codec> fmt::Debug for ReceiverStream<T, Codec> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("ReceiverStream").finish()
    }
}

impl<T, Codec, const BUFFER: usize> ReceiverStream<T, Codec, BUFFER>
where
    T: RemoteSend + Sync,
    Codec: codec::Codec,
{
    /// Creates a new `ReceiverStream`.
    pub fn new(rx: Receiver<T, Codec, BUFFER>) -> Self {
        Self { inner: ReusableBoxFuture::new(Self::make_future(rx)) }
    }

    async fn make_future(
        mut rx: Receiver<T, Codec, BUFFER>,
    ) -> (Result<T, RecvError>, Receiver<T, Codec, BUFFER>) {
        let result = rx.recv().await;
        (result, rx)
    }
}

impl<T: Clone, Codec, const BUFFER: usize> Stream for ReceiverStream<T, Codec, BUFFER>
where
    T: RemoteSend + Sync,
    Codec: codec::Codec,
{
    type Item = Result<T, StreamError>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context) -> Poll<Option<Self::Item>> {
        let (result, rx) = ready!(self.inner.poll(cx));
        self.inner.set(Self::make_future(rx));
        match result {
            Ok(value) => Poll::Ready(Some(Ok(value))),
            Err(RecvError::Closed) => Poll::Ready(None),
            Err(err) => Poll::Ready(Some(Err(StreamError::try_from(err).unwrap()))),
        }
    }
}

impl<T, Codec, const BUFFER: usize> Unpin for ReceiverStream<T, Codec, BUFFER> {}

impl<T, Codec, const BUFFER: usize> From<Receiver<T, Codec, BUFFER>> for ReceiverStream<T, Codec, BUFFER>
where
    T: RemoteSend + Sync,
    Codec: codec::Codec,
{
    fn from(recv: Receiver<T, Codec, BUFFER>) -> Self {
        Self::new(recv)
    }
}
