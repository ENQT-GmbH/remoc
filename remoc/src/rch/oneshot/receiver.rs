use futures::{ready, task::noop_waker, Future};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use std::{
    error::Error,
    fmt,
    pin::Pin,
    task::{Context, Poll},
};

use super::super::{base, buffer, mpsc};
use crate::{
    chmux,
    codec::{self},
    RemoteSend,
};

/// An error occurred during receiving over an oneshot channel.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum RecvError {
    /// Sender dropped without sending a value.
    Closed,
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
            Self::Closed => write!(f, "channel is closed"),
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

impl Error for RecvError {}

/// An error occurred during trying to receive over an oneshot channel.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum TryRecvError {
    /// No value has been received yet.
    Empty,
    /// Sender dropped without sending a value.
    Closed,
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
            Self::Empty => write!(f, "channel is empty"),
            Self::Closed => write!(f, "channel is closed"),
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

impl Error for TryRecvError {}

/// Receive a value from the associated sender.
///
/// Await this future to receive the value.
#[derive(Serialize, Deserialize)]
#[serde(bound(serialize = "T: RemoteSend, Codec: codec::Codec"))]
#[serde(bound(deserialize = "T: RemoteSend, Codec: codec::Codec"))]
pub struct Receiver<T, Codec = codec::Default>(pub(crate) mpsc::Receiver<T, Codec, buffer::Custom<1>>);

impl<T, Codec> fmt::Debug for Receiver<T, Codec> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("Receiver").finish_non_exhaustive()
    }
}

impl<T, Codec> Receiver<T, Codec>
where
    T: DeserializeOwned + Send + 'static,
    Codec: codec::Codec,
{
    /// Prevents the associated sender from sending a value.
    #[inline]
    pub fn close(&mut self) {
        self.0.close()
    }

    /// Attempts to receive a value.
    #[inline]
    pub fn try_recv(&mut self) -> Result<T, TryRecvError> {
        let waker = noop_waker();
        let mut cx = Context::from_waker(&waker);

        match self.0.poll_recv(&mut cx) {
            Poll::Ready(Ok(Some(v))) => Ok(v),
            Poll::Ready(Ok(None)) => Err(TryRecvError::Closed),
            Poll::Ready(Err(err)) => Err(err.into()),
            Poll::Pending => Err(TryRecvError::Empty),
        }
    }
}

impl<T, Codec> Future for Receiver<T, Codec>
where
    T: DeserializeOwned + Send + 'static,
    Codec: codec::Codec,
{
    type Output = Result<T, RecvError>;

    #[inline]
    fn poll(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
        match ready!(Pin::into_inner(self).0.poll_recv(cx)) {
            Ok(Some(v)) => Poll::Ready(Ok(v)),
            Ok(None) => Poll::Ready(Err(RecvError::Closed)),
            Err(err) => Poll::Ready(Err(err.into())),
        }
    }
}
