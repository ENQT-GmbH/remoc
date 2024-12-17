//! Worker thread pool.

use std::{
    collections::VecDeque,
    fmt,
    num::NonZero,
    sync::{Arc, LazyLock, Mutex},
    thread,
    thread::Thread,
};

use super::super::MutexExt;

/// Thread pool.
pub struct ThreadPool {
    max_workers: usize,
    inner: Arc<Mutex<Inner>>,
}

impl fmt::Debug for ThreadPool {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        f.debug_struct("ThreadPool").field("max_workers", &self.max_workers).finish_non_exhaustive()
    }
}

type Task = Box<dyn FnOnce() + Send + 'static>;

#[derive(Default)]
struct Inner {
    workers: Vec<Thread>,
    tasks: VecDeque<Task>,
    exit: bool,
    idle: usize,
}

impl ThreadPool {
    /// Creates a new thread pool of the specified maximum size.
    pub fn new(max_workers: NonZero<usize>) -> Self {
        Self { max_workers: max_workers.get(), inner: Arc::new(Mutex::new(Inner::default())) }
    }

    /// Enqueues a task on the thread pool.
    ///
    /// Returns an error if worker thread spawning failed.
    pub fn exec(&self, f: impl FnOnce() + Send + 'static) -> Result<(), std::io::Error> {
        let mut inner = self.inner.xlock().unwrap();
        inner.tasks.push_back(Box::new(f));

        // Check if we need to spawn a new worker.
        if inner.idle == 0 && inner.workers.len() < self.max_workers {
            let id = inner.workers.len() + 1;
            tracing::debug!("starting worker {id} / {}", self.max_workers);
            let hnd = {
                let inner = self.inner.clone();
                thread::Builder::new().name(format!("remoc worker {id}")).spawn(move || Self::worker(inner))
            }?;
            inner.workers.push(hnd.thread().clone());
        } else {
            for worker in &inner.workers {
                worker.unpark();
            }
        }

        Ok(())
    }

    /// Worker function.
    fn worker(inner: Arc<Mutex<Inner>>) {
        let mut idle = false;

        loop {
            let mut inner = inner.xlock().unwrap();
            if let Some(task) = inner.tasks.pop_front() {
                if idle {
                    inner.idle -= 1;
                    idle = false;
                }

                drop(inner);
                task();
            } else if inner.exit {
                break;
            } else {
                if !idle {
                    inner.idle += 1;
                    idle = true;
                }

                drop(inner);
                thread::park();
            }
        }
    }
}

impl Drop for ThreadPool {
    fn drop(&mut self) {
        let mut inner = self.inner.xlock().unwrap();

        inner.exit = true;

        for worker in &inner.workers {
            worker.unpark();
        }
    }
}

/// Default thread pool.
///
/// Uses at most one worker per CPU.
pub fn default() -> &'static ThreadPool {
    static INSTANCE: LazyLock<ThreadPool> = LazyLock::new(|| {
        let parallelism = thread::available_parallelism().unwrap_or_else(|err| {
            tracing::warn!("available parallelism is unknown: {err}");
            NonZero::new(1).unwrap()
        });
        ThreadPool::new(parallelism)
    });

    &INSTANCE
}
