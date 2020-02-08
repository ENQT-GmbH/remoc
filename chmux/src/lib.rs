//! # Channel multiplexer
//!
//! Multiplexes multiple channels over a single channel (or anything that implements Sink and Stream).
//!

mod channel;
mod client;
mod codec;
mod multiplexer;
mod number_allocator;
mod receive_buffer;
mod receiver;
mod send_lock;
mod sender;
mod server;

pub mod codecs;

pub use codec::{Serializer, Deserializer, CodecFactory};
pub use channel::Channel;
pub use client::{Client, ConnectError};
pub use multiplexer::{Cfg, MultiplexError, MultiplexMsg, Multiplexer};
pub use receiver::{ReceiveError, Receiver};
pub use sender::{HangupNotify, SendError, Sender};
pub use server::{Server, ServerError, RemoteConnectToServiceRequest};
