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
mod sender;
mod receiver;
mod client;
mod server;

pub mod codecs;


pub use multiplexer::{Cfg, Multiplexer, MultiplexError, MultiplexMsg};
pub use sender::{SendError, Sender};
pub use receiver::{Receiver, ReceiveError};
pub use client::{Client, ConnectError};
pub use server::{Server};
pub use channel::Channel;

