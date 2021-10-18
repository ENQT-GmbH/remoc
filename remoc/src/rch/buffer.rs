//! Static buffer size configuration for received channel-halves.
//!
//! This is used to specify the local buffer size (in items) via a generic type parameter
//! when the sender or receiver half of a channel is received.
//!
//! The default buffer size is 2 items.
//! It can be increased to improve performance, but this will also increase
//! the maximum amount of memory used per channel.
//! Setting the buffer size too high must be avoided, since this can lead to
//! the program running out of memory.

use serde::{Deserialize, Serialize};

/// Static buffer size.
pub trait Size: Send + Sync + 'static {
    /// Gets the buffer size.
    fn size() -> usize;
}

/// Default buffer size.
///
/// Current default buffer size is 2.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Default {}

impl Size for Default {
    fn size() -> usize {
        2
    }
}

/// Custom buffer size.
///
/// Use the const generic parameter to specify the size.
pub struct Custom<const SIZE: usize> {}

impl<const SIZE: usize> Size for Custom<SIZE> {
    fn size() -> usize {
        SIZE
    }
}
