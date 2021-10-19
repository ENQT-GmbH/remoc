#![forbid(unsafe_code)]
#![warn(missing_docs)]
#![cfg_attr(docsrs, feature(doc_cfg))]

//! ReMOC 🦑 — Remote multiplexed objects and channels
//!
//! # Crate features
//!
//! The modules of this crate are gated by crate features, as shown below.
//! For ease of use all features are enabled by default.
//! See the [codec module](codec) documentation on how to select a default codec.
//! The `full` feature enables all functionality but does not include a default codec.

pub mod prelude;

pub mod chmux;

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
