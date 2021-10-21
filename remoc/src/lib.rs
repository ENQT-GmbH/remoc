#![forbid(unsafe_code)]
#![warn(missing_docs)]
#![cfg_attr(docsrs, feature(doc_cfg))]

//! Remoc 🦑 — Remote multiplexed objects and channels
//!
//! # Physical transport
//!
//! All functionality in Remoc requires that a connection over a physical
//! transport is established.
//! The underlying transport can either be of packet type (implementing [Sink] and [Stream])
//! or a socket-like object (implementing [AsyncRead] and [AsyncWrite]).
//! In both cases it must be ordered and reliable.
//! That means that all packets must arrive in the order they have been sent
//! and no packets must be lost.
//! The maximum packet size can be limited, see [the configuration](Cfg) for that.
//!
//! [TCP] is an example of an underlying transport that is suitable.
//! But there are many more candidates, for example, [UNIX domain sockets],
//! [pipes between processes], [serial links], [Bluetooth L2CAP streams], etc.
//!
//! The [connect functions](Connect) are used to establish a
//! [base channel connection](rch::base) over a physical transport.
//! Then, additional channels can be opened by sending either the sender or receiver
//! half of them over the established base channel or another connected channel.
//! See the examples in the [remote channel module](rch) for details.
//!
//! [Sink]: futures::Sink
//! [Stream]: futures::Stream
//! [AsyncRead]: tokio::io::AsyncRead
//! [AsyncWrite]: tokio::io::AsyncWrite
//! [TCP]: https://docs.rs/tokio/1.12.0/tokio/net/struct.TcpStream.html
//! [UNIX domain sockets]: https://docs.rs/tokio/1.12.0/tokio/net/struct.UnixStream.html
//! [pipes between processes]: https://docs.rs/tokio/1.12.0/tokio/process/struct.Child.html
//! [serial links]: https://docs.rs/tokio-serial/5.4.1/tokio_serial/
//! [Bluetooth L2CAP streams]: https://docs.rs/bluer/0.10.4/bluer/l2cap/struct.Stream.html
//!
//! # Crate features
//!
//! The modules of Remoc are gated by crate features, as shown below.
//! For ease of use all features are enabled by default.
//! See the [codec module](codec) documentation on how to select a default codec.
//! The `full` feature enables all functionality but does not include a default codec.
//!

pub mod prelude;

pub mod chmux;
pub use chmux::Cfg;

#[cfg(feature = "serde")]
#[cfg_attr(docsrs, doc(cfg(feature = "serde")))]
pub mod codec;

#[cfg(feature = "rch")]
#[cfg_attr(docsrs, doc(cfg(feature = "rch")))]
pub mod rch;

#[cfg(feature = "rch")]
mod remote_send;
#[cfg(feature = "rch")]
#[cfg_attr(docsrs, doc(cfg(feature = "rch")))]
pub use remote_send::RemoteSend;

#[cfg(feature = "rch")]
mod connect;
#[cfg(feature = "rch")]
#[cfg_attr(docsrs, doc(cfg(feature = "rch")))]
pub use connect::{Connect, ConnectError};

#[cfg(feature = "rfn")]
#[cfg_attr(docsrs, doc(cfg(feature = "rfn")))]
pub mod rfn;

#[cfg(feature = "robj")]
#[cfg_attr(docsrs, doc(cfg(feature = "robj")))]
pub mod robj;

#[cfg(feature = "rtc")]
#[cfg_attr(docsrs, doc(cfg(feature = "rtc")))]
pub mod rtc;

#[cfg(any(feature = "rfn", feature = "robj"))]
mod provider;
#[cfg(any(feature = "rfn", feature = "robj"))]
pub use provider::Provider;

#[doc(hidden)]
#[cfg(feature = "rch")]
pub mod doctest;
