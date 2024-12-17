//! Executor for futures.
//!
//! On native platforms this uses Tokio.
//! On WebAssembly this executes Futures as Promises.

#[cfg(not(feature = "web"))]
mod native {
    pub mod task {
        pub use tokio::task::{spawn, spawn_blocking, JoinError, JoinHandle};
    }

    pub mod runtime {
        pub use tokio::runtime::Handle;
    }
}

#[cfg(not(feature = "web"))]
pub use native::*;

#[cfg(feature = "web")]
pub mod task;

#[cfg(feature = "web")]
pub mod runtime;

#[cfg(feature = "web")]
mod thread_pool;

#[cfg(feature = "web")]
mod sync_wrapper;

/// Whether blocking is allowed on this thread.
///
/// On native targets blocking is always allowed.
/// On web blocking is only allowed on worker threads.
#[inline]
pub fn is_blocking_allowed() -> bool {
    #[cfg(not(feature = "web"))]
    {
        true
    }

    #[cfg(feature = "web")]
    {
        use std::cell::LazyCell;
        use wasm_bindgen::{prelude::*, JsCast};

        #[wasm_bindgen]
        extern "C" {
            #[wasm_bindgen(js_name = WorkerGlobalScope)]
            pub type WorkerGlobalScope;
        }

        thread_local! {
            static ALLOWED: LazyCell<bool> = LazyCell::new(||
                js_sys::global().is_instance_of::<WorkerGlobalScope>()
            );
        }

        ALLOWED.with(|allowed| **allowed)
    }
}

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
