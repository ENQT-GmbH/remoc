//! A read/write lock that can be sent to remote endpoints.

mod msg;
mod owner;
#[allow(clippy::module_inception)]
mod rw_lock;

pub use owner::Owner;
pub use rw_lock::{CommitError, LockError, ReadGuard, ReadLock, RwLock, WriteGuard};
