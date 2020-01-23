//! # Channel multiplexer
//! 
//! Multiplexes multiple channels over a single channel (or anything that implements Sink and Stream).
//! 


mod channel;
mod codec;
mod number_allocator;
mod send_lock;
mod receive_buffer;
mod multiplexer;
mod raw_channel;
mod raw_sender;
mod raw_receiver;
mod receiver;
mod client;
mod server;
mod sender;

pub use multiplexer::{Cfg, Multiplexer, MultiplexRunError, MultiplexMsg};
pub use raw_sender::{RawSender, RawSendError};
pub use raw_receiver::{RawReceiver, RawReceiveError};
pub use raw_channel::RawChannel;
pub use client::{MultiplexerClient, MultiplexerConnectError};
pub use server::{MultiplexerServer};
pub use receiver::{Receiver, ReceiveError};
pub use sender::{Sender, SendError};