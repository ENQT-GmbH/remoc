//! Static buffer size configuration.

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
pub struct Custom<const SIZE: usize> {}

impl<const SIZE: usize> Size for Custom<SIZE> {
    fn size() -> usize {
        SIZE
    }
}