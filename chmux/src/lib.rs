//! # Channel multiplexer
//!
//! Multiplexes multiple channels over a single channel (or anything that implements Sink and Stream).
//!

#![warn(missing_docs)]

mod client;
mod credit;
mod listener;
mod msg;
mod multiplexer;
mod port_allocator;
mod receiver;
mod sender;

use std::{error::Error, fmt};

pub use client::{Client, Connect, ConnectError};
pub use listener::{Listener, ListenerError, ListenerStream, Request};
pub use msg::Cfg;
pub use multiplexer::Multiplexer;
pub use port_allocator::{PortAllocator, PortNumber};
pub use receiver::{DataBuf, ReceiveError, Received, Receiver, ReceiverStream};
pub use sender::{Closed, SendError, Sender, SenderSink, TrySendError};

/// Channel multiplexer protocol version.
pub const PROTOCOL_VERSION: u8 = 2;

/// Channel multiplexer error.
#[derive(Debug)]
pub enum MultiplexError<SinkError, StreamError> {
    /// An error was encountered while sending data to the transport sink.
    SinkError(SinkError),
    /// An error was encountered while receiving data from the transport stream.
    StreamError(StreamError),
    /// The transport stream was closed while multiplex channels were active or the
    /// multiplex client was not dropped.
    StreamClosed,
    /// The connection was reset by the remote endpoint.
    Reset,
    /// No messages where received over the configured connection timeout.
    Timeout,
    /// Too many ports specified in configuration.
    TooManyPorts,
    /// A multiplex protocol error occured.
    Protocol(String),
}

impl<SinkError, StreamError> fmt::Display for MultiplexError<SinkError, StreamError>
where
    SinkError: fmt::Display,
    StreamError: fmt::Display,
{
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Self::SinkError(err) => write!(f, "send error: {}", err),
            Self::StreamError(err) => write!(f, "receive error: {}", err),
            Self::StreamClosed => write!(f, "end of receive stream"),
            Self::Reset => write!(f, "connection reset"),
            Self::Timeout => write!(f, "connection timeout"),
            Self::TooManyPorts => write!(f, "too many ports configured"),
            Self::Protocol(err) => write!(f, "protocol error: {}", err),
        }
    }
}

impl<SinkError, StreamError> Error for MultiplexError<SinkError, StreamError>
where
    SinkError: Error,
    StreamError: Error,
{
}

/// Return if connection terminated.
///
/// Argument must be of type `Result<_, SendError>` or `Result<_, ReceiveError>`.
///
/// Returns from the current function (with no return value) if the
/// argument is an error result due to a closed channel or terminated
/// multiplexer. Otherwise the result is passed unmodified.
#[macro_export]
macro_rules! term {
    ($exp:expr) => {
        match $exp {
            Ok(v) => Ok(v),
            Err(err) if err.is_terminated() => return,
            Err(err) => Err(err),
        }
    };
}

/// Ignore errors due to connection terminated.
///
/// Argument must be of type `Result<(), SendError>`.
#[macro_export]
macro_rules! ignore_term {
    ($exp:expr) => {
        match $exp {
            Ok(()) => Ok(()),
            Err(err) if err.is_terminated() => Ok(()),
            Err(err) => Err(err),
        }
    };
}
