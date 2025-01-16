//! Async executive for futures.
//!
//! On native platforms this uses Tokio.
//! On JavaScript this executes Futures as Promises.

#[cfg(not(feature = "js"))]
mod native;

#[cfg(not(feature = "js"))]
pub use native::*;

#[cfg(feature = "js")]
mod js;

#[cfg(feature = "js")]
pub use js::*;

pub use task::spawn;

/// Whether threads are available and working on this platform.
#[inline]
pub fn are_threads_available() -> bool {
    use std::sync::LazyLock;

    static AVAILABLE: LazyLock<bool> = LazyLock::new(|| {
        let res = std::thread::Builder::new().name("remoc thread test".into()).spawn(|| ());
        match res {
            Ok(hnd) => match hnd.join() {
                Ok(()) => true,
                Err(payload) => {
                    tracing::warn!(?payload, "test thread panicked, streaming (de)serialization disabled");
                    false
                }
            },
            Err(os_error) => {
                tracing::warn!(%os_error, "threads not available, streaming (de)serialization disabled");
                false
            }
        }
    });

    *AVAILABLE
}
