//! Remote async functions and closures.

use serde::{Deserialize, Serialize};
use std::{error::Error, fmt};

use crate::{
    chmux,
    rsync::{oneshot, remote},
};

pub mod msg;
mod rfn_const;
mod rfn_mut;
mod rfn_once;

pub use rfn_const::{RFn, RFnProvider};
pub use rfn_mut::{RFnMut, RFnMutProvider};
pub use rfn_once::{RFnOnce, RFnOnceProvider};

/// An error occured during calling a remote function.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum CallError {
    /// Provider was dropped or function panicked.
    Dropped,
    /// Receiving from a remote endpoint failed.
    RemoteReceive(remote::RecvError),
    /// Connecting a sent channel failed.
    RemoteConnect(chmux::ConnectError),
    /// Listening for a connection from a received channel failed.
    RemoteListen(chmux::ListenerError),
}

impl fmt::Display for CallError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Self::Dropped => write!(f, "provider dropped or function panicked"),
            Self::RemoteReceive(err) => write!(f, "receive error: {}", err),
            Self::RemoteConnect(err) => write!(f, "connect error: {}", err),
            Self::RemoteListen(err) => write!(f, "listen error: {}", err),
        }
    }
}

impl From<oneshot::RecvError> for CallError {
    fn from(err: oneshot::RecvError) -> Self {
        match err {
            oneshot::RecvError::Closed => Self::Dropped,
            oneshot::RecvError::RemoteReceive(err) => Self::RemoteReceive(err),
            oneshot::RecvError::RemoteConnect(err) => Self::RemoteConnect(err),
            oneshot::RecvError::RemoteListen(err) => Self::RemoteListen(err),
        }
    }
}

impl Error for CallError {}
