//! ReMOC🐙 — Remote multiplexed objects and channels
//!
//! ## Transport constraints
//!
//! Both [connect_framed] and [connect_io] spawn the channel multiplexer onto a separate task and
//! thus the transport must have static lifetime and be [Send] and [Sync].
//! To avoid this, you can create and run the channel multiplexer manually.
//! To do so, instance [ChMux](chmux::ChMux) directly and invoke [remote::connect](rsync::remote::connect)
//! to create the initial channel.

pub mod chmux;
pub mod codec;
mod connect;
pub mod rch;
pub mod rfn;
pub mod robj;
pub mod rtc;

pub use connect::{connect_framed, connect_io, ConnectError};
