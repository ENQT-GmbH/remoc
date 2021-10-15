#![deny(unsafe_code)]
#![warn(missing_docs)]
#![cfg_attr(docsrs, feature(doc_cfg))]

//! ReMOC🐙 — Remote multiplexed objects and channels
//!

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
pub use connect::{connect_framed, connect_io, ConnectError};

#[cfg(feature = "rfn")]
#[cfg_attr(docsrs, doc(cfg(feature = "rfn")))]
pub mod rfn;

#[cfg(feature = "robj")]
#[cfg_attr(docsrs, doc(cfg(feature = "robj")))]
pub mod robj;

#[cfg(feature = "rtc")]
#[cfg_attr(docsrs, doc(cfg(feature = "rtc")))]
pub mod rtc;
