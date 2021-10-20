//! Remote channels.
//!
//! This module contains channels that can be used to exchange data of
//! arbitrary type with a remote endpoint.
//!
//! The data type must be serializable and deserializable by [serde] and
//! [sendable across thread boundaries](Send).
//! The [RemoteSend](crate::RemoteSend) trait is implemented automatically
//! for all types that fulfill these requirements.
//!
//! # Establishing a channel connection with a remote endpoint
//!
//! Except for a [base channel](base), a channel connection is established by sending
//! the Sender or Receiver half of a channel to a remote endpoint over an already
//! existing channel.
//! The transmitted channel-half can also be part of a larger object, such as a struct, tuple or enum.
//! Most channel types can even be forwarded over multiple connections.
//!
//! The primary purpose of a [base channel](base) is to provide an initial channel after
//! establishing a connection over a physical transport.
//! In most cases there is no need to create it directly and you will probably only use it
//! after it has been returned from an [initial connect function](crate::Connect).
//!
//! # Channel types
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
//!
//! # Acknowledgements and connection latency
//!
//! The channels do not wait for acknowledgement of transmitted values.
//! Thus, the next value can be queued for transmission before the previous value
//! has been received by the remote endpoint.
//! Hence, throughput is unaffected by the connection roundtrip time.
//!
//! A successful send does not guarantee that the transmitted value will be received
//! by the remote endpoint.
//! However, values cannot be lost intermittently, i.e. if a transmission error
//! occurs on the underlying physical connection the whole [chmux] connection will
//! be terminated eventually and sending will fail.
//!
//! If you need confirmation for every sent value, consider sending a [oneshot::Sender]`<()>`
//! along with it (for example as a tuple).
//! The remote endpoint can then send back an empty message as confirmation over the oneshot channel
//! after it has processed the received value.
//!
//! # Size considerations
//!
//! The size of the objects exchanged is not limited.
//! If the object is small enough it is first serialized into a buffer and then sent as
//! one message to the remote endpoint.
//! For larger objects, a serialization thread is spawned and the message is sent
//! chunk-by-chunk to avoid the need for a large temporary memory buffer.
//! Deserialization is also performed on-the-fly as data is received for large objects.
//!
//! Large values are always sent in chunks, so that one channel does not block other channels
//! for a long period of time.
//!
//! To avoid denial of service attacks when you exchange data with untrusted remote endpoints,
//! make sure that an object cannot grow to infinite size during deserialization.
//! For example, avoid using containers of unlimited size like [Vec] or [HashMap](std::collections::HashMap)
//! in your data structure or limit their maximum size during deserialization by applying
//! the `#[serde(deserialize_with="...")]` attribute to the fields containing them.
//!

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

/// Back channel message that receiver has been closed.
pub(crate) const BACKCHANNEL_MSG_CLOSE: u8 = 0x01;

/// Back channel message that error has occurred.
pub(crate) const BACKCHANNEL_MSG_ERROR: u8 = 0x02;

/// Remote sending error.
#[derive(Clone)]
pub(crate) enum RemoteSendError {
    /// Send error.
    Send(base::SendErrorKind),
    /// Connect error.
    Connect(chmux::ConnectError),
    /// Listen for connect error.
    Listen(chmux::ListenerError),
    /// Forwarding error.
    Forward,
}
