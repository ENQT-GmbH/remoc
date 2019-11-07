//! # Channel multiplexer
//! 
//! Multiplexes multiple channels over a single channel (or anything that implements Sink and Stream).
//! 


mod number_allocator;
mod send_lock;
mod receive_buffer;
mod multiplexer;
mod sender;
mod receiver;
mod client;
mod server;

pub use multiplexer::{Cfg, Multiplexer, MultiplexRunError, MultiplexMsg};
pub use sender::{ChannelSender, ChannelSendError};
pub use receiver::{ChannelReceiver, ChannelReceiveError};
pub use client::{MultiplexerClient, MultiplexerConnectError};
pub use server::{MultiplexerServer};
