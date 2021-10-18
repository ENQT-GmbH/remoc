//! Remote channels.
//!
//! This module contains channels that can be used to exchange data of
//! arbitrary type with a remote endpoint.
//!
//! The data type must be serializable and deserialize by [serde] and
//! [sendable across thread boundaries](Send).
//! The [RemoteSend](crate::RemoteSend) trait is implemented automatically
//! for all types that fulfill these requirements.
//!
//! ## Establishing a channel connection with a remote endpoint
//!
//! Except for a [base channel](base), a channel connection is established by sending
//! the Sender or Receiver half of a channel to a remote endpoint over an already
//! existing channel.
//! The transmitted channel-half can also be part of a larger object, such as a struct or enum.
//! Most channel types can even be forwarded over multiple connections.
//!
//! The primary purpose of a [base channel](base) is to provide an initial channel after
//! establishing a connection over a physical transport.
//! In most cases there is no need to create it directly and you will probably only use it
//! after it has been returned from an [initial connect function](crate::Connect).
//!
//! ## Channel types
//!
//! [Broadcast](broadcast), [MPSC](mpsc), [oneshot] and [watch] channels are closely
//! modelled after the channels found in [tokio::sync].
//! They also work when both halves of the channel are local and can be forwarded
//! over multiple connections.
//!
//! An [local/remote channel](lr) is a more restricted version of an [MPSC](mpsc) channel.
//! It does not support forwarding and exactly one half of it must be on a remote endpoint.
//! Its benefit is, that it does not spawn an async task and thus uses less resources.
//! When in doubt use an [MPSC channel](mpsc) instead.
//!
//! A [binary channel](bin) can be used to exchange binary data over a channel.
//! It skips serialization and deserialization and thus is more efficient for binary data,
//! especially when using text codecs such as JSON.
//! However, it does not support forwarding and exactly one half of it must be on a remote
//! endpoint.

use serde::{Deserialize, Serialize};
use std::{error::Error, fmt};

use crate::chmux;

mod interlock;

pub mod base;
pub mod bin;
pub mod broadcast;
pub mod buffer;
pub mod lr;
pub mod mpsc;
pub mod oneshot;
pub mod watch;

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

pub(crate) const BACKCHANNEL_MSG_CLOSE: u8 = 0x01;
pub(crate) const BACKCHANNEL_MSG_ERROR: u8 = 0x02;

#[derive(Clone)]
pub(crate) enum RemoteSendError {
    Send(base::SendErrorKind),
    Connect(chmux::ConnectError),
    Listen(chmux::ListenerError),
    Forward,
}
