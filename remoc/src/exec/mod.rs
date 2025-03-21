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
pub async fn are_threads_available() -> bool {
    use tokio::sync::{oneshot, OnceCell};

    static AVAILABLE: OnceCell<bool> = OnceCell::const_new();
    *AVAILABLE
        .get_or_init(|| async move {
            tracing::trace!("spawning test thread");

            let (tx, rx) = oneshot::channel();
            let res = std::thread::Builder::new().name("remoc thread test".into()).spawn(move || {
                tracing::trace!("test thread started");
                let _ = tx.send(());
            });

            match res {
                Ok(_) => {
                    tracing::trace!("waiting for test thread");
                    match rx.await {
                        Ok(()) => {
                            tracing::trace!("threads are available");
                            true
                        }
                        Err(_) => {
                            tracing::warn!("test thread failed, streaming (de)serialization disabled");
                            false
                        }
                    }
                }
                Err(os_error) => {
                    tracing::warn!(%os_error, "threads not available, streaming (de)serialization disabled");
                    false
                }
            }
        })
        .await
}
