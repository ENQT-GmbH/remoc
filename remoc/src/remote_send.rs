use serde::{de::DeserializeOwned, Serialize};

/// An object that is sendable to a remote endpoint.
///
/// This trait is automatically implemented for objects that are
/// serializable, deserializable and sendable between threads.
#[cfg_attr(docsrs, doc(cfg(feature = "rch")))]
pub trait RemoteSend: Send + Serialize + DeserializeOwned + 'static {}

impl<T> RemoteSend for T where T: Send + Serialize + DeserializeOwned + 'static {}
