//! Time.

#![allow(unsafe_code)]

use js_sys::Function;
use std::{
    fmt,
    future::{Future, IntoFuture},
    pin::Pin,
    sync::{Arc, Mutex},
    task::{Context, Poll, Waker},
    time::Duration,
};
use wasm_bindgen::{prelude::*, JsCast};
use web_sys::{Window, WorkerGlobalScope};

/// Future returned by [`sleep`].
pub struct Sleep {
    inner: Arc<Mutex<SleepInner>>,
    timeout_id: i32,
    _callback: Closure<dyn FnMut()>,
}

// Implement Send + Sync for target without threads.
#[cfg(all(target_family = "wasm", not(target_feature = "atomics")))]
unsafe impl Send for Sleep {}
#[cfg(all(target_family = "wasm", not(target_feature = "atomics")))]
unsafe impl Sync for Sleep {}

impl fmt::Debug for Sleep {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("Sleep").field("timeout_id", &self.timeout_id).finish()
    }
}

#[derive(Default)]
struct SleepInner {
    fired: bool,
    waker: Option<Waker>,
}

impl Sleep {
    fn new(duration: Duration) -> Self {
        let inner = Arc::new(Mutex::new(SleepInner::default()));

        let callback = {
            let inner = inner.clone();
            Closure::new(move || {
                let mut inner = inner.lock().unwrap();
                inner.fired = true;
                if let Some(waker) = inner.waker.take() {
                    waker.wake();
                }
            })
        };

        let timeout = duration.as_millis().try_into().expect("sleep duration overflow");
        let timeout_id = Self::register_timeout(callback.as_ref().unchecked_ref(), timeout);

        Self { inner, timeout_id, _callback: callback }
    }

    fn register_timeout(handler: &Function, timeout: i32) -> i32 {
        let global = js_sys::global();

        if let Some(window) = global.dyn_ref::<Window>() {
            window.set_timeout_with_callback_and_timeout_and_arguments_0(handler, timeout).unwrap()
        } else if let Some(worker) = global.dyn_ref::<WorkerGlobalScope>() {
            worker.set_timeout_with_callback_and_timeout_and_arguments_0(handler, timeout).unwrap()
        } else {
            panic!("unsupported JavaScript global: {global:?}");
        }
    }

    fn unregister_timeout(id: i32) {
        let global = js_sys::global();

        if let Some(window) = global.dyn_ref::<Window>() {
            window.clear_timeout_with_handle(id);
        } else if let Some(worker) = global.dyn_ref::<WorkerGlobalScope>() {
            worker.clear_timeout_with_handle(id);
        } else {
            panic!("unsupported JavaScript global: {global:?}");
        }
    }
}

impl Future for Sleep {
    type Output = ();

    /// Waits until the sleep duration has elapsed.
    fn poll(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
        let mut inner = self.inner.lock().unwrap();

        if inner.fired {
            return Poll::Ready(());
        }

        inner.waker = Some(cx.waker().clone());
        Poll::Pending
    }
}

impl Drop for Sleep {
    fn drop(&mut self) {
        let inner = self.inner.lock().unwrap();
        if !inner.fired {
            Self::unregister_timeout(self.timeout_id);
        }
    }
}

/// Waits until `duration` has elapsed.
pub fn sleep(duration: Duration) -> Sleep {
    Sleep::new(duration)
}

pub mod error {
    use super::*;

    /// Timeout elapsed.
    #[derive(Debug, Clone)]
    pub struct Elapsed {}

    impl fmt::Display for Elapsed {
        fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
            write!(f, "timeout elapsed")
        }
    }

    impl std::error::Error for Elapsed {}
}

/// Future returned by [`timeout`].
pub struct Timeout<F> {
    sleep: Sleep,
    future: F,
}

impl<F> fmt::Debug for Timeout<F> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("Timeout").finish()
    }
}

impl<F> Future for Timeout<F>
where
    F: Future,
{
    type Output = Result<F::Output, error::Elapsed>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
        let future = unsafe { self.as_mut().map_unchecked_mut(|this| &mut this.future) };
        if let Poll::Ready(res) = future.poll(cx) {
            return Poll::Ready(Ok(res));
        }

        let sleep = unsafe { self.as_mut().map_unchecked_mut(|this| &mut this.sleep) };
        if let Poll::Ready(()) = sleep.poll(cx) {
            return Poll::Ready(Err(error::Elapsed {}));
        }

        Poll::Pending
    }
}

/// Requires a `Future` to complete before the specified duration has elapsed.
pub fn timeout<F>(duration: Duration, future: F) -> Timeout<F::IntoFuture>
where
    F: IntoFuture,
{
    Timeout { sleep: sleep(duration), future: future.into_future() }
}
