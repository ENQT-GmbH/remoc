//! JavaScript async executive.

use std::cell::LazyCell;
use wasm_bindgen::JsCast;
use web_sys::WorkerGlobalScope;

pub mod runtime;
pub mod task;
pub mod time;

mod sync_wrapper;
mod thread_pool;

/// Whether blocking is allowed on this thread.
///
/// On JavaScript blocking is only allowed on worker threads.
#[inline]
pub fn is_blocking_allowed() -> bool {
    thread_local! {
        static ALLOWED: LazyCell<bool> = LazyCell::new(||
            js_sys::global().is_instance_of::<WorkerGlobalScope>()
        );
    }

    ALLOWED.with(|allowed| **allowed)
}
