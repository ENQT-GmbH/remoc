//! Native async executor using Tokio.

pub mod runtime {
    pub use tokio::runtime::Handle;
}

pub mod task {
    use std::future::Future;

    pub use tokio::task::{spawn, spawn_blocking, JoinError, JoinHandle};

    /// Runs a future to completion.
    #[track_caller]
    pub fn block_on<F: Future>(future: F) -> F::Output {
        let rt = tokio::runtime::Builder::new_current_thread().build().unwrap();
        rt.block_on(future)
    }
}

pub mod time {
    pub use tokio::time::{sleep, timeout, Sleep, Timeout};

    pub mod error {
        pub use tokio::time::error::Elapsed;
    }
}

/// Whether blocking is allowed on this thread.
///
/// On native targets blocking is always allowed.
#[inline]
pub fn is_blocking_allowed() -> bool {
    true
}
