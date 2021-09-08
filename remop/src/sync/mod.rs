use crate::chmux;
use serde::{Deserialize, Serialize};
use std::{error::Error, fmt};

mod interlock;
pub mod lr;
pub mod mpsc;
pub mod raw;
mod remote;

/// Error connecting a remote channel.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ConnectError {
    /// The corresponding sender or receiver has been dropped.
    Dropped,
    /// Error initiating chmux connection.
    Connect(chmux::ConnectError),
    /// Error listening for or accepting chmux connection.
    Listen(chmux::ListenerError),
}

impl fmt::Display for ConnectError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            ConnectError::Dropped => write!(f, "other part was dropped"),
            ConnectError::Connect(err) => write!(f, "connect error: {}", err),
            ConnectError::Listen(err) => write!(f, "listen error: {}", err),
        }
    }
}

impl Error for ConnectError {}
