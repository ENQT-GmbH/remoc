use futures::{ready, task::noop_waker, Future};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use std::{
    error::Error,
    fmt,
    pin::Pin,
    task::{Context, Poll},
};

use super::super::{mpsc, remote, RemoteSend};
use crate::{chmux, codec::CodecT};

/// An error occured during receiving over an oneshot channel.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum ReceiveError {
    /// Sender dropped without sending a value.
    Closed,
    /// Receiving from a remote endpoint failed.
    RemoteReceive(remote::ReceiveError),
    /// Connecting a sent channel failed.
    RemoteConnect(chmux::ConnectError),
    /// Listening for a connection from a received channel failed.
    RemoteListen(chmux::ListenerError),
}

impl fmt::Display for ReceiveError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Self::Closed => write!(f, "channel is closed"),
            Self::RemoteReceive(err) => write!(f, "receive error: {}", err),
            Self::RemoteConnect(err) => write!(f, "connect error: {}", err),
            Self::RemoteListen(err) => write!(f, "listen error: {}", err),
        }
    }
}

impl From<mpsc::ReceiveError> for ReceiveError {
    fn from(err: mpsc::ReceiveError) -> Self {
        match err {
            mpsc::ReceiveError::RemoteReceive(err) => Self::RemoteReceive(err),
            mpsc::ReceiveError::RemoteConnect(err) => Self::RemoteConnect(err),
            mpsc::ReceiveError::RemoteListen(err) => Self::RemoteListen(err),
        }
    }
}

impl Error for ReceiveError {}

/// An error occured during trying to receive over an oneshot channel.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum TryReceiveError {
    /// No value has been received yet.
    Empty,
    /// Sender dropped without sending a value.
    Closed,
    /// Receiving from a remote endpoint failed.
    RemoteReceive(remote::ReceiveError),
    /// Connecting a sent channel failed.
    RemoteConnect(chmux::ConnectError),
    /// Listening for a connection from a received channel failed.
    RemoteListen(chmux::ListenerError),
}

impl fmt::Display for TryReceiveError {
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

impl From<mpsc::ReceiveError> for TryReceiveError {
    fn from(err: mpsc::ReceiveError) -> Self {
        match err {
            mpsc::ReceiveError::RemoteReceive(err) => Self::RemoteReceive(err),
            mpsc::ReceiveError::RemoteConnect(err) => Self::RemoteConnect(err),
            mpsc::ReceiveError::RemoteListen(err) => Self::RemoteListen(err),
        }
    }
}

impl Error for TryReceiveError {}

/// Receive a value from the associated sender.
#[derive(Debug, Serialize, Deserialize)]
#[serde(bound(serialize = "T: RemoteSend, Codec: CodecT"))]
#[serde(bound(deserialize = "T: RemoteSend, Codec: CodecT"))]
pub struct Receiver<T, Codec>(pub(crate) mpsc::Receiver<T, Codec, 1>);

impl<T, Codec> Receiver<T, Codec>
where
    T: DeserializeOwned + Send + 'static,
    Codec: CodecT,
{
    /// Prevents the associated sender from sending a value.
    pub fn close(&mut self) {
        self.0.close()
    }

    /// Attempts to receive a value.
    pub fn try_recv(&mut self) -> Result<T, TryReceiveError> {
        let waker = noop_waker();
        let mut cx = Context::from_waker(&waker);

        match self.0.poll_recv(&mut cx) {
            Poll::Ready(Ok(Some(v))) => Ok(v),
            Poll::Ready(Ok(None)) => Err(TryReceiveError::Closed),
            Poll::Ready(Err(err)) => Err(err.into()),
            Poll::Pending => Err(TryReceiveError::Empty),
        }
    }
}

impl<T, Codec> Future for Receiver<T, Codec>
where
    T: DeserializeOwned + Send + 'static,
    Codec: CodecT,
{
    type Output = Result<T, ReceiveError>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
        match ready!(Pin::into_inner(self).0.poll_recv(cx)) {
            Ok(Some(v)) => Poll::Ready(Ok(v)),
            Ok(None) => Poll::Ready(Err(ReceiveError::Closed)),
            Err(err) => Poll::Ready(Err(err.into())),
        }
    }
}
