//! Remote trait calling.

use std::{error::Error, fmt, sync::Arc};

use crate::{
    chmux,
    rch::{mpsc, oneshot, remote},
};

/// Attribute that must be applied on traits and their implementations that
/// contain async functions.
pub use async_trait::async_trait;

/// Denotes a trait as remotely callable.
///
/// It adds the provided method `serve` to the trait, which serves the object using
/// a `chmux::Server`.
/// All methods in the service trait definition must be async.
/// The server trait implementation must use `[async_trait::async_trait]` attribute.
///
/// The chmux messages are of type `MultiplexMsg<Content>` where `Content` is
/// the name of the trait suffixed with `Service`.
///
/// Additionally a client proxy struct named using the same name suffixed with `Client`
/// is generated.
/// It is constructed using the method `bind` from a `chmux::Client`.
///
/// # Attributes
/// If the `#[no_cancel]` attribute is applied on a method, it will run to completion,
/// even if the client cancels the request by dropping the Future.
///
/// Serde attributes on arguments are moved to the respective field of the request
/// enum.
///
/// # Example
///
/// ```ignore
/// pub enum IncreaseError {
///     Overflow,
///     Call(CallError),
/// }
///
/// impl From<CallError> for IncreaseError {
///     fn from(err: CallError) -> Self { Self::Call(err) }
/// }
///
/// #[remote]
/// pub trait Counter<Codec> {
///     async fn value(&self) -> Result<u32, CallError>;
///     async fn watch(&self) -> Result<watch::Receiver<u32, Codec>, CallError>;
///     #[no_cancel]
///     async fn increase(&mut self, #[serde(default)] by: u32) -> Result<(), IncreaseError>;
/// }
/// ```
///
pub use remoc_macro::remote;

/// Call a method on a remotable trait failed.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CallError {
    /// Server has been dropped.
    Dropped,
    /// Sending to a remote endpoint failed.
    RemoteSend(remote::SendErrorKind),
    /// Receiving from a remote endpoint failed.
    RemoteReceive(remote::RecvError),
    /// Connecting a sent channel failed.
    RemoteConnect(chmux::ConnectError),
    /// Listening for a received channel failed.
    RemoteListen(chmux::ListenerError),
    /// Forwarding at a remote endpoint to another remote endpoint failed.
    RemoteForward,
}

impl fmt::Display for CallError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Self::Dropped => write!(f, "provider dropped or function panicked"),
            Self::RemoteSend(err) => write!(f, "send error: {}", err),
            Self::RemoteReceive(err) => write!(f, "receive error: {}", err),
            Self::RemoteConnect(err) => write!(f, "connect error: {}", err),
            Self::RemoteListen(err) => write!(f, "listen error: {}", err),
            Self::RemoteForward => write!(f, "forwarding error"),
        }
    }
}

impl Error for CallError {}

impl<T> From<mpsc::SendError<T>> for CallError {
    fn from(err: mpsc::SendError<T>) -> Self {
        match err {
            mpsc::SendError::Closed(_) => Self::Dropped,
            mpsc::SendError::RemoteSend(err) => Self::RemoteSend(err),
            mpsc::SendError::RemoteConnect(err) => Self::RemoteConnect(err),
            mpsc::SendError::RemoteListen(err) => Self::RemoteListen(err),
            mpsc::SendError::RemoteForward => Self::RemoteForward,
        }
    }
}

impl From<oneshot::RecvError> for CallError {
    fn from(err: oneshot::RecvError) -> Self {
        match err {
            oneshot::RecvError::Closed => Self::Dropped,
            oneshot::RecvError::RemoteReceive(err) => Self::RemoteReceive(err),
            oneshot::RecvError::RemoteConnect(err) => Self::RemoteConnect(err),
            oneshot::RecvError::RemoteListen(err) => Self::RemoteListen(err),
        }
    }
}

/// A request from client to server.
#[doc(hidden)]
#[derive(Serialize, Deserialize)]
pub enum Req<V, R, M> {
    /// Request by value (self).
    Value(V),
    /// Request by reference (&self).
    Ref(R),
    /// Request by mutable reference (&mut self).
    RefMut(M),
}

/// Base trait for the server of a remotable trait.
pub trait ServerBase {
    /// The client type, which can be sent to a remote endpoint.
    type Client;
}

/// A server of a remotable trait taking the target object by value.
#[async_trait(?Send)]
pub trait Server<T, Codec>: ServerBase
where
    Self: Sized,
{
    /// Creates a new server instance for the target object.
    fn new(target: T, request_buffer: usize) -> (Self, Self::Client);

    /// Serves the target object.
    ///
    /// Serving ends when the client is dropped or a method taking self by value
    /// is called. In the first case, the target object is returned and, in the
    /// second case, None is returned.
    async fn serve(self) -> Option<T>;
}

/// A server of a remotable trait taking the target object by reference.
#[async_trait(?Send)]
pub trait ServerRef<'target, Target, Codec>: ServerBase
where
    Self: Sized,
{
    /// Creates a new server instance for the target object.
    fn new(target: &'target Target, request_buffer: usize) -> (Self, Self::Client);

    /// Serves the target object.
    ///
    /// Serving ends when the client is dropped.
    async fn serve(self);
}

/// A server of a remotable trait taking the target object by mutable reference.
#[async_trait(?Send)]
pub trait ServerRefMut<'target, Target, Codec>: ServerBase
where
    Self: Sized,
{
    /// Creates a new server instance for the target object.
    fn new(target: &'target mut Target, request_buffer: usize) -> (Self, Self::Client);

    /// Serves the target object.
    ///
    /// Serving ends when the client is dropped.
    async fn serve(self);
}

/// A server of a remotable trait taking the target object by shared reference.
#[async_trait]
pub trait ServerShared<Target, Codec>: ServerBase
where
    Self: Sized,
    Self::Client: Clone,
{
    /// Creates a new server instance for a shared reference to the target object.
    fn new(target: Arc<Target>, request_buffer: usize) -> (Self, Self::Client);

    /// Serves the target object.
    ///
    /// If `spawn` is true, remote calls are executed in parallel by spawning a task per call.
    ///
    /// Serving ends when the client is dropped.
    async fn serve(self, spawn: bool);
}

/// A server of a remotable trait taking the target object by shared mutable reference.
#[async_trait]
pub trait ServerSharedMut<T, Codec>: ServerBase
where
    Self: Sized,
{
    /// Creates a new server instance for a shared mutable reference to the target object.
    fn new(target: Arc<LocalRwLock<T>>, request_buffer: usize) -> (Self, Self::Client);

    /// Serves the target object.
    ///
    /// If `spawn` is true, remote calls taking a `&self` reference are executed
    /// in parallel by spawning a task per call.
    /// Remote calls taking a `&mut self` reference are serialized by obtaining a write lock.
    ///
    /// Serving ends when the client is dropped.
    async fn serve(self, spawn: bool);
}

// Re-exports for proc macro usage.
#[doc(hidden)]
pub use serde::{Deserialize, Serialize};
#[doc(hidden)]
pub use tokio::sync::mpsc as local_mpsc;
#[doc(hidden)]
pub use tokio::sync::RwLock as LocalRwLock;
#[doc(hidden)]
pub use tokio::{select, spawn};

#[doc(hidden)]
/// Log message that receiving a request failed for proc macro.
pub fn receiving_request_failed(err: mpsc::RecvError) {
    log::warn!("Receiving request failed: {}", &err)
}
