//! Remotely observable collections for use with [remoc].
//!
//! This crate provides collections that emit an event for each change.
//! This event stream can be sent to a remote endpoint via a remote channel,
//! where it can be either processed event-wise or a mirrored collection can
//! be built from it.
//!

pub mod hashmap;
pub mod hashset;
pub mod vec;

use remoc::prelude::*;
use serde::{Deserialize, Serialize};
use std::{error::Error, fmt};

/// An error occurred during sending an event for an observable HashMap change.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum SendError {
    /// Sending to a remote endpoint failed.
    RemoteSend(rch::base::SendErrorKind),
    /// Connecting a sent channel failed.
    RemoteConnect(chmux::ConnectError),
    /// Listening to a received channel failed.
    RemoteListen(chmux::ListenerError),
    /// Forwarding at a remote endpoint to another remote endpoint failed.
    RemoteForward,
}

impl fmt::Display for SendError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Self::RemoteSend(err) => write!(f, "send error: {}", err),
            Self::RemoteConnect(err) => write!(f, "connect error: {}", err),
            Self::RemoteListen(err) => write!(f, "listen error: {}", err),
            Self::RemoteForward => write!(f, "forwarding error"),
        }
    }
}

impl Error for SendError {}

impl<T> TryFrom<rch::broadcast::SendError<T>> for SendError {
    type Error = rch::broadcast::SendError<T>;

    fn try_from(err: rch::broadcast::SendError<T>) -> Result<Self, Self::Error> {
        match err {
            rch::broadcast::SendError::RemoteSend(err) => Ok(Self::RemoteSend(err)),
            rch::broadcast::SendError::RemoteConnect(err) => Ok(Self::RemoteConnect(err)),
            rch::broadcast::SendError::RemoteListen(err) => Ok(Self::RemoteListen(err)),
            rch::broadcast::SendError::RemoteForward => Ok(Self::RemoteForward),
            other @ rch::broadcast::SendError::Closed(_) => Err(other),
        }
    }
}

impl<T> TryFrom<rch::mpsc::SendError<T>> for SendError {
    type Error = rch::mpsc::SendError<T>;

    fn try_from(err: rch::mpsc::SendError<T>) -> Result<Self, Self::Error> {
        match err {
            rch::mpsc::SendError::RemoteSend(err) => Ok(Self::RemoteSend(err)),
            rch::mpsc::SendError::RemoteConnect(err) => Ok(Self::RemoteConnect(err)),
            rch::mpsc::SendError::RemoteListen(err) => Ok(Self::RemoteListen(err)),
            rch::mpsc::SendError::RemoteForward => Ok(Self::RemoteForward),
            other @ rch::mpsc::SendError::Closed(_) => Err(other),
        }
    }
}

/// An error occurred during receiving an event or initial value of an observed HashMap.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum RecvError {
    /// The observed collection was dropped before `done` was called on it.
    Closed,
    /// The receiver lagged behind, so that the the send buffer size has reached its limit.
    ///
    /// Try increasing the send buffer specified when calling `subscribe` on the
    /// observed collection.
    Lagged,
    /// Receiving from a remote endpoint failed.
    RemoteReceive(rch::base::RecvError),
    /// Connecting a sent channel failed.
    RemoteConnect(chmux::ConnectError),
    /// Listening for a connection from a received channel failed.
    RemoteListen(chmux::ListenerError),
}

impl fmt::Display for RecvError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Self::Closed => write!(f, "observed HashMap was dropped"),
            Self::Lagged => write!(f, "observation lagged behind"),
            Self::RemoteReceive(err) => write!(f, "receive error: {}", err),
            Self::RemoteConnect(err) => write!(f, "connect error: {}", err),
            Self::RemoteListen(err) => write!(f, "listen error: {}", err),
        }
    }
}

impl Error for RecvError {}

impl From<rch::broadcast::RecvError> for RecvError {
    fn from(err: rch::broadcast::RecvError) -> Self {
        match err {
            rch::broadcast::RecvError::Closed => Self::Closed,
            rch::broadcast::RecvError::Lagged => Self::Lagged,
            rch::broadcast::RecvError::RemoteReceive(err) => Self::RemoteReceive(err),
            rch::broadcast::RecvError::RemoteConnect(err) => Self::RemoteConnect(err),
            rch::broadcast::RecvError::RemoteListen(err) => Self::RemoteListen(err),
        }
    }
}

impl From<rch::mpsc::RecvError> for RecvError {
    fn from(err: rch::mpsc::RecvError) -> Self {
        match err {
            rch::mpsc::RecvError::RemoteReceive(err) => Self::RemoteReceive(err),
            rch::mpsc::RecvError::RemoteConnect(err) => Self::RemoteConnect(err),
            rch::mpsc::RecvError::RemoteListen(err) => Self::RemoteListen(err),
        }
    }
}

/// Sends an event.
pub(crate) fn send_event<E, Codec>(tx: &rch::broadcast::Sender<E, Codec>, on_err: &dyn Fn(SendError), event: E)
where
    Codec: remoc::codec::Codec,
    E: RemoteSend + Clone,
{
    match tx.send(event) {
        Ok(()) => (),
        Err(err) if err.is_disconnected() => (),
        Err(err) => match err.try_into() {
            Ok(err) => (on_err)(err),
            Err(_) => unreachable!(),
        },
    }
}

/// Default handler for sending errors.
pub(crate) fn default_on_err(err: SendError) {
    tracing::warn!("sending failed: {}", err);
}
