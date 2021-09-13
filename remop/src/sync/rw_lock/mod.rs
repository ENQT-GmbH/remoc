//! A read/write lock that can be sent to remote endpoints.

pub mod msg;
mod owner;
mod rwlock;

pub use owner::Owner;
pub use rwlock::{CommitError, LockError, ReadGuard, ReadLock, RwLock, WriteGuard};
