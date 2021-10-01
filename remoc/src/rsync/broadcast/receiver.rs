use futures::task::noop_waker;
use serde::{Deserialize, Serialize};
use std::{
    error::Error,
    fmt,
    task::{Context, Poll},
};

use super::{
    super::{
        mpsc,
        remote::{self},
    },
    BroadcastMsg,
};
use crate::{chmux, codec::CodecT, rsync::RemoteSend};

/// An error occured during receiving over a broadcast channel.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum RecvError {
    /// There are no more active senders implying no further messages will ever be sent.
    Closed,
    /// The receiver lagged too far behind.
    ///
    /// Attempting to receive again will return the oldest message still retained by the channel.
    Lagged,
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

impl Error for RecvError {}

/// An error occured during trying to receive over a broadcast channel.
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
    RemoteReceive(remote::RecvError),
    /// Connecting a sent channel failed.
    RemoteConnect(chmux::ConnectError),
    /// Listening for a connection from a received channel failed.
    RemoteListen(chmux::ListenerError),
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

impl Error for TryRecvError {}

/// Receiving-half of the broadcast channel.
///
/// Can be sent over a remote channel.
#[derive(Serialize, Deserialize)]
#[serde(bound(serialize = "T: RemoteSend, Codec: CodecT"))]
#[serde(bound(deserialize = "T: RemoteSend, Codec: CodecT"))]
pub struct Receiver<T, Codec, const BUFFER: usize> {
    rx: mpsc::Receiver<BroadcastMsg<T>, Codec, BUFFER>,
}

impl<T, Codec, const BUFFER: usize> fmt::Debug for Receiver<T, Codec, BUFFER> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("Receiver").finish_non_exhaustive()
    }
}

impl<T, Codec, const BUFFER: usize> Receiver<T, Codec, BUFFER>
where
    T: RemoteSend,
    Codec: CodecT,
{
    pub(crate) fn new(rx: mpsc::Receiver<BroadcastMsg<T>, Codec, BUFFER>) -> Self {
        Self { rx }
    }

    /// Receives the next value for this receiver.
    pub async fn recv(&mut self) -> Result<T, RecvError> {
        match self.rx.recv().await {
            Ok(Some(BroadcastMsg::Value(value))) => Ok(value),
            Ok(Some(BroadcastMsg::Lagged)) => Err(RecvError::Lagged),
            Ok(None) => Err(RecvError::Closed),
            Err(err) => Err(err.into()),
        }
    }

    /// Attempts to return a pending value on this receiver without awaiting.    
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
