//! Remote async functions and closures.
//!
//! This module contains wrappers around async functions and closures to make them
//! callable from a remote endpoint.
//! Since Rust differentiates between immutable, mutable and by-value functions,
//! remote wrappers for all three kinds of functions are provided here.
//!
//! All wrappers take a single function argument, but you can use a tuple as
//! argument to simulate standard multi-argument call syntax.
//! The argument and return type of the function must be [remote sendable](crate::RemoteSend).
//!
//! Each wrapper spawns an async tasks that processes function execution requests
//! from the remote endpoint.
//!
//! ## Usage
//!
//! Create a wrapper locally and send it to a remote endpoint, for example over a
//! channel from the [rch](crate::rch) module.
//! Then use the `call` method on the remote endpoint to remotely invoke the local function.
//!
//! Note that the function is executed locally.
//! Only the argument and return value are transmitted from and to the remote endpoint.
//!
//! ## Return type
//!
//! Since a remote function call can fail due to connection problems, the return type
//! of the wrapped function must always be of the [Result] type.
//! Thus your function should return a [Result] type with an error type that can
//! convert from [CallError] and thus absorb the remote calling error.
//! If you return a different type the `call` method will not be available on the wrapper,
//! but you can still use the `try_call` method, which wraps the result into a [Result] type.
//!
//! ## Providers
//!
//! Optionally you can use the `provided` method of each wrapper to obtain a
//! provider for each remote function wrapper.
//! This allows you to drop the wrapped function without relying upon the
//! remote endpoint for that.
//! This is especially useful when you connect to untrusted remote endpoints
//! that could try to obtain and keep a large number of remote function wrappers to
//! perform a denial of service attack by exhausting your memory.
//!

use serde::{Deserialize, Serialize};
use std::{error::Error, fmt};

use crate::{
    chmux,
    rch::{base, oneshot},
};

mod msg;
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
    RemoteReceive(base::RecvError),
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
