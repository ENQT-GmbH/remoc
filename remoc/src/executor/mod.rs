//! Async executor for futures.
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

/// Mutex extensions for WebAssembly support.
pub trait MutexExt<T> {
    /// Acquires a mutex, blocking the current thread until it is able to do so.
    ///
    /// If blocking is not allowed on the current thread, it spins until the
    /// lock is acquired.
    fn xlock(&self) -> std::sync::LockResult<std::sync::MutexGuard<'_, T>>;
}

impl<T> MutexExt<T> for std::sync::Mutex<T> {
    #[inline]
    fn xlock(&self) -> std::sync::LockResult<std::sync::MutexGuard<'_, T>> {
        if is_blocking_allowed() {
            return self.lock();
        }

        // Spin until lock is acquired.
        loop {
            match self.try_lock() {
                Ok(guard) => return Ok(guard),
                Err(std::sync::TryLockError::Poisoned(p)) => return Err(p),
                Err(std::sync::TryLockError::WouldBlock) => (),
            }
        }
    }
}

pub use task::spawn;
