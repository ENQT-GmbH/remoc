//! Remote trait calling.
//!
//! This module allows calling of methods on an object located on a remote endpoint via a trait.
//!
//! By tagging a trait with the [remote attribute](remote), server and client types
//! are generated for that trait.
//! The client type contains an automatically generated implementation of the trait.
//! Each call is encoded into a request and send to the server.
//! The server accepts requests from the client and calls the requested trait method on an object
//! implementing that trait located on the server.
//! It then transmits the result back to the client.
//!
//! # Client type
//!
//! Assuming that the trait is called `Trait`, the client will be called `TraitClient`.
//!
//! The client type implements the trait and is [remote sendable](crate::RemoteSend) over
//! a [remote channel](crate::rch) or any other means to a remote endpoint.
//! All methods called on the client will be forwarded to the server and executed there.
//!
//! The client type also implements the [Client] trait which provides a notification
//! when the connection to the server has been lost.
//!
//! If the trait takes the receiver only by reference (`&self`) the client is [clonable](Clone).
//!
//! # Server types
//!
//! Assuming the trait is called `Trait`, the server names will all start with `TraitServer`.
//!
//! Depending on whether the trait takes the receiver by value (`self`), by reference (`&self`) or
//! by mutable reference (`&mut self`) different server types are generated:
//!
//!   * `TraitServer` is always generated,
//!   * `TraitServerRefMut` and `TraitServerSharedMut` are generated when the receiver is
//!      *never* taken by value,
//!   * `TraitServerRef` and `TraitServerShared` are generated when the receiver is
//!     *never* taken by value and mutable reference.
//!
//! The purpose of these server types is as follows:
//!
//!   * `TraitServer` implements [Server] and takes the target object by value. It will
//!     consume the target value when a trait method taking the receiver by value is invoked.
//!   * `TraitServerRef` implements [ServerRef] and takes a reference to the target value.
//!   * `TraitServerRefMut` implements [ServerRefMut] and takes a mutable reference to the target value.
//!   * `TraitServerShared` implements [ServerShared] and takes an [Arc] to the target value.
//!     It can execute client requests in parallel.
//!   * `TraitServerSharedMut` implements [ServerSharedMut] and takes an [Arc] to a local
//!     [RwLock](LocalRwLock) holding the target object.
//!     It can execute const client requests in parallel and mutable requests sequentially.
//!
//! # Usage
//!
//! Tag your trait with the [remote attribute](remote).
//! Call `new()` on a server type to create a server and corresponding client instance for a
//! target object, which must implement the trait.
//! Send the client to a remote endpoint and then call `serve()` on the server instance to
//! start processing requests by the client.
//!
//! # Error handling
//!
//! Since a remote trait call can fail due to connection problems, the return type
//! of all trait functions must always be of the [Result] type.
//! The error type must be able to convert from [CallError] and thus absorb the remote calling error.
//!
//! There is no timeout imposed on a remote call, but the underlying [chmux] connection
//! [pings the remote endpoint](chmux::Cfg::connection_timeout) by default.
//! If the underlying connection fails, all remote calls will automatically fail.
//! You can wrap remote calls using [tokio::time::timeout] if you need to use
//! per-call timeouts.
//!
//! # Cancellation
//!
//! If the client drops the future of a call while it is executing or the connection is interrupted
//! the trait function on the server is automatically cancelled at the next `await` point.
//! You can apply the `#[no_cancel]` attribute to a method to always run it to completion.
//!
//! # Forward and backward compatibility
//!
//! All request arguments are packed into an enum case named after the function.
//! Each argument corresponds to a field with the same name.
//! Thus it is always safe to add new arguments at the end and apply the `#[serde(default)]`
//! attribute to them.
//! Arguments that are passed by the client but are unknown to the server will be silently discarded.
//!
//! Also, new functions can be added to the trait without breaking backward compatibility.
//! Calling a non-existent function (for example when the client is newer than the server) will
//! result in a error, but the server will continue serving.
//! It is thus safe to just attempt to call a server function to see if it is available.
//!
//! # Alternatives
//!
//! If you just need to expose a function remotely using [remote functions](crate::rfn) is simpler.
//!
//! # Example
//!
//! This is a short example only; a fully worked example with client and server split into
//! their own crates is available in the
//! [examples directory](https://github.com/ENQT-GmbH/remoc/tree/master/examples/rtc).
//! This can also be used as a template to get started quickly.
//!
//! In the following example a trait `Counter` is defined and marked as remotely callable.
//! It is implemented on the `CounterObj` struct.
//! The server creates a `CounterObj` and obtains a `CounterServerSharedMut` and `CounterClient` for it.
//! The `CounterClient` is then sent to the client, which receives it and calls
//! trait methods on it.
//!
//! ```
//! use std::sync::Arc;
//! use tokio::sync::RwLock;
//! use remoc::prelude::*;
//! use remoc::rtc::CallError;
//!
//! // Custom error type that can convert from CallError.
//! #[derive(Debug, serde::Serialize, serde::Deserialize)]
//! pub enum IncreaseError {
//!     Overflow,
//!     Call(CallError),
//! }
//!
//! impl From<CallError> for IncreaseError {
//!     fn from(err: CallError) -> Self {
//!         Self::Call(err)
//!     }
//! }
//!
//! // Trait defining remote service.
//! #[rtc::remote]
//! pub trait Counter {
//!     async fn value(&self) -> Result<u32, CallError>;
//!
//!     async fn watch(&mut self) -> Result<rch::watch::Receiver<u32>, CallError>;
//!
//!     #[no_cancel]
//!     async fn increase(&mut self, #[serde(default)] by: u32)
//!         -> Result<(), IncreaseError>;
//! }
//!
//! // Server implementation object.
//! pub struct CounterObj {
//!     value: u32,
//!     watchers: Vec<rch::watch::Sender<u32>>,
//! }
//!
//! impl CounterObj {
//!     pub fn new() -> Self {
//!         Self { value: 0, watchers: Vec::new() }
//!     }
//! }
//!
//! // Server implementation of trait methods.
//! #[rtc::async_trait]
//! impl Counter for CounterObj {
//!     async fn value(&self) -> Result<u32, CallError> {
//!         Ok(self.value)
//!     }
//!
//!     async fn watch(&mut self) -> Result<rch::watch::Receiver<u32>, CallError> {
//!         let (tx, rx) = rch::watch::channel(self.value);
//!         self.watchers.push(tx);
//!         Ok(rx)
//!     }
//!
//!     async fn increase(&mut self, by: u32) -> Result<(), IncreaseError> {
//!         match self.value.checked_add(by) {
//!             Some(new_value) => self.value = new_value,
//!             None => return Err(IncreaseError::Overflow),
//!         }
//!
//!         for watch in &self.watchers {
//!             let _ = watch.send(self.value);
//!         }
//!
//!         Ok(())
//!     }
//! }
//!
//! // This would be run on the client.
//! async fn client(mut rx: rch::base::Receiver<CounterClient>) {
//!     let mut remote_counter = rx.recv().await.unwrap().unwrap();
//!     let mut watch_rx = remote_counter.watch().await.unwrap();
//!
//!     assert_eq!(remote_counter.value().await.unwrap(), 0);
//!
//!     remote_counter.increase(20).await.unwrap();
//!     assert_eq!(remote_counter.value().await.unwrap(), 20);
//!
//!     remote_counter.increase(45).await.unwrap();
//!     assert_eq!(remote_counter.value().await.unwrap(), 65);
//!
//!     assert_eq!(*watch_rx.borrow().unwrap(), 65);
//! }
//!
//! // This would be run on the server.
//! async fn server(mut tx: rch::base::Sender<CounterClient>) {
//!     let mut counter_obj = Arc::new(RwLock::new(CounterObj::new()));
//!
//!     let (server, client) = CounterServerSharedMut::new(counter_obj, 1);
//!     tx.send(client).await.unwrap();
//!     server.serve(true).await;
//! }
//! # tokio_test::block_on(remoc::doctest::client_server(server, client));
//! ```
//!

use futures::{future::BoxFuture, Future, FutureExt};
use std::{
    error::Error,
    fmt,
    pin::Pin,
    sync::Arc,
    task::{Context, Poll},
};

use crate::{
    chmux,
    rch::{base, mpsc, oneshot},
};

/// Attribute that must be applied on all implementations of a trait
/// marked with the [remote] attribute.
///
/// This is a re-export from the [mod@async_trait] crate.
pub use async_trait::async_trait;

/// Denotes a trait as remotely callable and generate a client and servers for it.
///
/// See [module-level documentation](self) for details and examples.
///
/// This generates the client and server structs for the trait.
/// If the trait is called `Trait` the client will be called `TraitClient` and
/// the name of the servers will start with `TraitServer`.
///
/// # Requirements
///
/// Each trait method must be async and have return type `Result<T, E>` where `T` and `E` are
/// [remote sendable](crate::RemoteSend) and `E` must implemented [`From`]`<`[`CallError`]`>`.
/// All arguments must also be [remote sendable](crate::RemoteSend).
/// Of course, you can use all remote types from Remoc in your arguments and return type,
/// for example [remote channels](crate::rch) and [remote objects](crate::rch).
///
/// This uses async_trait, so you must apply the [macro@async_trait] attribute on
/// all implementation of the trait.
///
/// Since the generated code relies on [Tokio](tokio) macros, you must add a dependency
/// to Tokio in your `Cargo.toml`.
///
/// # Generics and lifetimes
///
/// The trait may be generic with constraints on the generic arguments.
/// You will probably need to constrain them on [RemoteSend](crate::RemoteSend).
/// Method definitions within the remote trait may use generic arguments from the trait
/// definition, but must not introduce generic arguments in the method definition.
///
/// Lifetimes are not allowed on remote traits and their methods.
///
/// # Attributes
///
/// If the `#[no_cancel]` attribute is applied on a trait method, it will run to completion,
/// even if the client cancels the request by dropping the future.
///
/// All [serde field attributes](https://serde.rs/field-attrs.html) `#[serde(...)]`
/// are allowed on the arguments of the functions.
/// They will be transferred to the respective field of the request struct that will
/// be send to the server when the method is called by the client.
/// This can be used to customize serialization and provide defaults for forward and backward
/// compatibility.
///
pub use remoc_macro::remote;

/// Call a method on a remotable trait failed.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CallError {
    /// Server has been dropped.
    Dropped,
    /// Sending to a remote endpoint failed.
    RemoteSend(base::SendErrorKind),
    /// Receiving from a remote endpoint failed.
    RemoteReceive(base::RecvError),
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

/// Client of a remotable trait.
pub trait Client {
    /// Returns the current capacity of the channel for sending requests to
    /// the server.
    ///
    /// Zero is returned when the server has been dropped or the connection
    /// has been lost.
    fn capacity(&self) -> usize;

    /// Returns a future that completes when the server or client has been
    /// dropped or the connection between them has been lost.
    ///
    /// In this case no more requests from this client will succeed.
    fn closed(&self) -> Closed;

    /// Returns whether the server has been dropped or the connection to it
    /// has been lost.
    fn is_closed(&self) -> bool;
}

/// A future that completes when the server or client has been dropped
/// or the connection between them has been lost.
///
/// This can be obtained via [Client::closed].
pub struct Closed(BoxFuture<'static, ()>);

impl fmt::Debug for Closed {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_tuple("Closed").finish()
    }
}

impl Closed {
    #[doc(hidden)]
    pub fn new(fut: impl Future<Output = ()> + Send + 'static) -> Self {
        Self(fut.boxed())
    }
}

impl Future for Closed {
    type Output = ();

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
        self.as_mut().0.poll_unpin(cx)
    }
}

/// Base trait shared between all server variants of a remotable trait.
pub trait ServerBase {
    /// The client type, which can be sent to a remote endpoint.
    type Client: Client;
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
pub use tokio::sync::broadcast as local_broadcast;
#[doc(hidden)]
pub use tokio::sync::mpsc as local_mpsc;
#[doc(hidden)]
pub use tokio::sync::RwLock as LocalRwLock;
#[doc(hidden)]
pub use tokio::{select, spawn};

/// Log message that receiving a request failed for proc macro.
#[doc(hidden)]
pub fn receiving_request_failed(err: mpsc::RecvError) {
    tracing::warn!(err = ?err, "receiving RTC request failed")
}

/// Broadcast sender with no subscribers.
#[doc(hidden)]
pub fn empty_client_drop_tx() -> local_broadcast::Sender<()> {
    local_broadcast::channel(1).0
}
