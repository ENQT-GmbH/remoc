//! Remote objects.
//!
//! This module provides functionality to access and transmit data remotely.
//! All types presented here can be sent over [remote channels](crate::rch)
//! or used as arguments or return types of [remote functions](crate::rfn) or in
//! [remote traits](crate::rtc).
//!

pub mod handle;
pub mod lazy;
pub mod lazy_blob;
pub mod rw_lock;
