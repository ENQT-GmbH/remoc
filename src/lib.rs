//! Remotely observable collections for use with [remoc].
//!
//! This crate provides collections that emit an event for each change.
//! This event stream can be sent to a remote endpoint via a remote channel,
//! where a mirrored collection can be built from it.
//!

pub mod hashmap;
pub mod vec;
