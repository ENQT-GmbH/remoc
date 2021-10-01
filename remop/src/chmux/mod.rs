//! # Channel multiplexer
//!
//! Multiplexes multiple channels over a single channel (or anything that implements Sink and Stream).
//!

#![deny(unsafe_code)]
#![warn(missing_docs)]

use std::{error::Error, fmt};

mod cfg;
mod client;
mod credit;
mod listener;
mod msg;
mod mux;
mod port_allocator;
mod receiver;
mod sender;

pub use cfg::{Cfg, PortsExhausted};
pub use client::{Client, Connect, ConnectError};
pub use listener::{Listener, ListenerError, ListenerStream, Request};
pub use mux::ChMux;
pub use port_allocator::{PortAllocator, PortNumber};
pub use receiver::{DataBuf, Received, Receiver, ReceiverStream, RecvAnyError, RecvChunkError, RecvError};
pub use sender::{ChunkSender, Closed, SendError, Sender, SenderSink, TrySendError};

/// Channel multiplexer protocol version.
pub const PROTOCOL_VERSION: u8 = 2;

/// Channel multiplexer error.
#[derive(Debug)]
pub enum ChMuxError<SinkError, StreamError> {
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
    /// A multiplex protocol error occured.
    Protocol(String),
}

impl<SinkError, StreamError> fmt::Display for ChMuxError<SinkError, StreamError>
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
            Self::Protocol(err) => write!(f, "protocol error: {}", err),
        }
    }
}

impl<SinkError, StreamError> Error for ChMuxError<SinkError, StreamError>
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
