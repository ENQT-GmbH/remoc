//! A read/write lock that shares a consistent view of a value over multiple remote endpoints.
//!
//! A remote read/write lock allows multiple remote endpoints to share a common value
//! that can be read and written by each endpoint.
//! Multiple endpoints can acquire simultaneous read access to the value.
//! Write access is exclusive and can only be held by one endpoint at a time.
//! This ensures that all endpoints always have a consist view of the shared value.
//! The shared value is always stored on the endpoint that created the
//! [RwLock owner](Owner).
//!
//! Each [RwLock] caches the current value locally until it is invalidated.
//! Thus, subsequent read operations are cheap.
//! However, a write operation requires several round trips between the owner
//! and the remote endpoints, thus its performance is limited by the physical connection
//! latency.
//!
//! ## Usage
//!
//! [Create an RwLock owner](Owner::new) and use [Owner::rw_lock] to acquire
//! read/write locks that can be send to remote endpoints, for example over a
//! [remote channel](crate::rch).
//! The remote endpoints can then use [RwLock::read] and [RwLock::write] to obtain
//! read or write access respectively.
//! When the [owner](Owner) is dropped, all locks become invalid and the value
//! is dropped.
//!
//! ## Alternatives
//!
//! If you require to broadcast value to multiple endpoints that just require read
//! access, a [watch channel](crate::rch::watch) might be a simpler and better option.
//!

mod msg;
mod owner;
#[allow(clippy::module_inception)]
mod rw_lock;

pub use owner::Owner;
pub use rw_lock::{CommitError, LockError, ReadGuard, ReadLock, RwLock, WriteGuard};
