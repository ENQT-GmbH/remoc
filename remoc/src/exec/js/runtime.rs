//! The runtime.

use futures::FutureExt;
use std::{future::Future, panic};
use tokio::sync::oneshot;

use super::{
    sync_wrapper::SyncWrapper,
    task::{JoinError, JoinErrorRepr, JoinHandle},
    thread_pool,
};

/// Handle to the virtual runtime.
#[derive(Debug, Clone)]
pub struct Handle;

impl Handle {
    /// Returns a Handle view over the currently running Runtime.
    pub fn current() -> Self {
        Self
    }

    /// Spawns a future onto the browser.
    #[track_caller]
    pub fn spawn<F>(&self, future: F) -> JoinHandle<F::Output>
    where
        F: Future + 'static,
        F::Output: 'static,
    {
        let (result_tx, result_rx) = oneshot::channel();
        let (abort_tx, abort_rx) = oneshot::channel();

        wasm_bindgen_futures::spawn_local(async move {
            let res = tokio::select! {
                biased;
                res = panic::AssertUnwindSafe(future).catch_unwind() =>
                    res.map_err(|payload| JoinError(JoinErrorRepr::Panicked(SyncWrapper::new(payload)))),
                Ok(()) = abort_rx => Err(JoinError(JoinErrorRepr::Aborted)),
            };
            let _ = result_tx.send(res);
        });

        JoinHandle { result_rx: Box::pin(result_rx), abort_tx: Some(abort_tx) }
    }

    /// Runs the provided function on an thread pool dedicated to blocking operations.
    #[track_caller]
    pub fn spawn_blocking<F, R>(&self, f: F) -> JoinHandle<R>
    where
        F: FnOnce() -> R + Send + 'static,
        R: Send + 'static,
    {
        let (result_tx, result_rx) = oneshot::channel();

        if let Err(err) = thread_pool::default().exec(move || {
            let res = panic::catch_unwind(panic::AssertUnwindSafe(f))
                .map_err(|payload| JoinError(JoinErrorRepr::Panicked(SyncWrapper::new(payload))));
            let _ = result_tx.send(res);
        }) {
            let (result_tx, result_rx) = oneshot::channel();
            result_tx.send(Err(JoinError(JoinErrorRepr::Spawn(err)))).unwrap_or_else(|_| unreachable!());
            return JoinHandle { result_rx: Box::pin(result_rx), abort_tx: None };
        }

        JoinHandle { result_rx: Box::pin(result_rx), abort_tx: None }
    }
}
