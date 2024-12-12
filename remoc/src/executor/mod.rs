//! Executor for futures.
//!
//! On native platforms this uses Tokio.
//! On WebAssembly this executes Futures as Promises.

#[cfg(not(target_family = "wasm"))]
mod native {
    pub mod task {
        pub use tokio::task::{spawn, spawn_blocking, JoinError, JoinHandle};
    }

    pub mod runtime {
        pub use tokio::runtime::Handle;
    }
}

#[cfg(not(target_family = "wasm"))]
pub use native::*;

#[cfg(target_family = "wasm")]
pub mod task;

#[cfg(target_family = "wasm")]
pub mod runtime;

#[cfg(target_family = "wasm")]
mod thread_pool;

#[cfg(target_family = "wasm")]
mod sync_wrapper;

/// Whether blocking is allowed on this thread.
///
/// On native targets blocking is always allowed.
/// On WebAssembly blocking is only allowed on worker threads.
#[inline]
pub fn blocking_allowed() -> bool {
    #[cfg(not(target_family = "wasm"))]
    {
        true
    }

    #[cfg(target_family = "wasm")]
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
        if blocking_allowed() {
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
