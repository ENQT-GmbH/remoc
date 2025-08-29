//! Remote trait calling.
//!
//! This module allows calling of methods on an object located on a remote endpoint via a trait.
//!
//! By tagging a trait with the [remote attribute](remote), server, client and request receiver
//! types are generated for that trait.
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
//! To force the client to be clonable, even if it takes the receiver by mutable reference (`&mut self`),
//! specify the `clone` argument to the [remote attribute](remote).
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
//!     *never* taken by value,
//!   * `TraitServerRef` and `TraitServerShared` are generated when the receiver is
//!     *never* taken by value and mutable reference.
//!
//! The purpose of these server types is as follows:
//!
//!   * server implementations with [`Send`] + [`Sync`] requirement on the target object (recommended):
//!     * `TraitServer` implements [Server] and takes the target object by value. It will
//!       consume the target value when a trait method taking the receiver by value is invoked.
//!     * `TraitServerShared` implements [ServerShared] and takes an [Arc] to the target value.
//!       It can execute client requests in parallel.
//!       The generated [`ServerShared::serve`] implementation returns a future that implements [`Send`].
//!     * `TraitServerSharedMut` implements [ServerSharedMut] and takes an [Arc] to a local
//!       [RwLock](LocalRwLock) holding the target object.
//!       It can execute const client requests in parallel and mutable requests sequentially.
//!       The generated [`ServerSharedMut::serve`] implementation returns a future that implements [`Send`].
//!   * server implementations with no [`Send`] + [`Sync`] requirement on the target object:
//!     * `TraitServerRef` implements [ServerRef] and takes a reference to the target value.
//!     * `TraitServerRefMut` implements [ServerRefMut] and takes a mutable reference to the target value.
//!
//! If unsure, you probably want to use `TraitServerSharedMut`, even when the target object will
//! only be accessed by a single client.
//!
//! # Request receiver type
//!
//! Assuming the trait is called `Trait`, the request receiver will be called `TraitReqReceiver`.
//!
//! The request receiver is also a server. However, instead of invoking the trait methods
//! on a target object, it allows you to process each request as a message and send the result
//! via a oneshot reply channel.
//!
//! See [ReqReceiver] for details.
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
//!     server.serve(true).await.unwrap();
//! }
//! # tokio_test::block_on(remoc::doctest::client_server(server, client));
//! ```
//!

use futures::{Future, FutureExt};
use std::{
    error::Error,
    fmt,
    pin::Pin,
    sync::Arc,
    task::{Context, Poll},
};
use tokio_util::sync::ReusableBoxFuture;

use crate::{
    RemoteSend, chmux, codec, exec,
    rch::{SendingError, SendingErrorKind, base, mpsc, oneshot},
};

/// Denotes a trait as remotely callable and generate a client and servers for it.
///
/// See [module-level documentation](self) for details and examples.
///
/// This generates the client, server and request receiver structs for the trait.
/// If the trait is called `Trait` the client will be called `TraitClient` and
/// the name of the servers will start with `TraitServer`. The request receiver
/// will be called `TraitReqReceiver`.
///
/// # Requirements
///
/// Each trait method must be either be
///
///   * an `async fn` and have return type `Result<T, E>`,
///   * a `fn` and have return type `impl Future<Output = Result<T, E>> + Send`,
///
/// where `T` and `E` are [remote sendable](crate::RemoteSend) and `E` must
/// implemented [`From`]`<`[`CallError`]`>`.
/// All arguments must also be [remote sendable](crate::RemoteSend).
/// Of course, you can use all remote types from Remoc in your arguments and return type,
/// for example [remote channels](crate::rch) and [remote objects](crate::rch).
///
/// Since the generated code relies on [Tokio](tokio) macros, you must add a dependency
/// to Tokio in your `Cargo.toml`.
///
/// # Generics and lifetimes
///
/// The trait may be generic with constraints on the generic arguments.
/// You will probably need to constrain them on [RemoteSend].
/// Method definitions within the remote trait may use generic arguments from the trait
/// definition, but must not introduce generic arguments in the method definition.
///
/// Lifetimes are not allowed on remote traits and their methods.
///
/// # Default implementations of methods
///
/// Default implementations of methods may be provided.
/// However, this requires specifying [`Send`] and [`Sync`] as supertraits of the remote trait.
///
/// # Attributes
///
/// If the `clone` argument is specified (by invoking the attribute macro as `#[remoc::rtc::remote(clone)]`),
/// the generated `TraitClient` will even be [clonable](std::clone::Clone) when the trait contains
/// methods taking the receiver by mutable reference (`&mut self`).
/// In this case the client can invoke more than one mutable method simultaneously; however,
/// the execution on the server will be serialized through locking.
///
/// If the `async_trait` argument is specified (by invoking the attribute macro as `#[remoc::rtc::remote(async_trait)]`),
/// the remote trait will be processed through the [`#[async_trait] macro`](https://docs.rs/async-trait), enabling
/// `dyn` dispatch. You must then include `async-trait` as a dependency in your `Cargo.toml` and apply the
/// `#[async_trait::async_trait]` attribute on all implementations of the trait.
///
/// The `server(...)` argument allows to limit the generated server variants.
/// Supported variants are: `Value`, `Ref`, `RefMut`, `Shared`, `SharedMut`, `ReqReceiver`.
/// Multiple variants can be specified as a comma-separated list.
/// For example, when `#[remoc::rtc::remote(server(SharedMut))]` is applied to `trait Trait` only the
/// `TraitServerSharedMut` server will be generated.
/// If unspecified, all server variants are generated.
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
    /// Processing request failed.
    ///
    /// The server may have been dropped or it may have panicked.
    /// Sending the response may have failed on the server-side.
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
            Self::Dropped => write!(f, "processing request failed"),
            Self::RemoteSend(err) => write!(f, "send error: {err}"),
            Self::RemoteReceive(err) => write!(f, "receive error: {err}"),
            Self::RemoteConnect(err) => write!(f, "connect error: {err}"),
            Self::RemoteListen(err) => write!(f, "listen error: {err}"),
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

    /// The maximum allowed size of a request in bytes.
    fn max_request_size(&self) -> usize;

    /// Sets the maximum allowed size of a request in bytes.
    ///
    /// This does not change the maximum request size the server will accept
    /// if this client has been received from a remote endpoint.
    fn set_max_request_size(&mut self, max_request_size: usize);

    /// The maximum allowed size of a reply in bytes.
    fn max_reply_size(&self) -> usize;

    /// Sets the maximum allowed size of a reply in bytes.
    fn set_max_reply_size(&mut self, max_reply_size: usize);
}

/// A future that completes when the server or client has been dropped
/// or the connection between them has been lost.
///
/// This can be obtained via [Client::closed].
pub struct Closed(ReusableBoxFuture<'static, ()>);

impl fmt::Debug for Closed {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_tuple("Closed").finish()
    }
}

impl Closed {
    #[doc(hidden)]
    pub fn new(fut: impl Future<Output = ()> + Send + 'static) -> Self {
        Self(ReusableBoxFuture::new(fut))
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

    /// Determines what should happen on the server-side if receiving an RTC
    /// request fails.
    ///
    /// The default is to ignore the receive error.
    fn set_on_req_receive_error(&mut self, on_req_receive_error: OnReqReceiveError);
}

/// A server of a remotable trait taking the target object by value.
pub trait Server<Target, Codec>: ServerBase
where
    Self: Sized,
{
    /// Creates a new server instance for the target object.
    fn new(target: Target, request_buffer: usize) -> (Self, Self::Client);

    /// Serves the target object.
    ///
    /// Serving ends when the client is dropped or a method taking self by value
    /// is called. In the first case, the target object is returned and, in the
    /// second case, None is returned.
    fn serve(self) -> impl Future<Output = (Option<Target>, Result<(), ServeError>)>;
}

/// A server of a remotable trait taking the target object by reference.
pub trait ServerRef<'target, Target, Codec>: ServerBase
where
    Self: Sized,
{
    /// Creates a new server instance for the target object.
    fn new(target: &'target Target, request_buffer: usize) -> (Self, Self::Client);

    /// Serves the target object.
    ///
    /// Serving ends when the client is dropped.
    fn serve(self) -> impl Future<Output = Result<(), ServeError>>;
}

/// A server of a remotable trait taking the target object by mutable reference.
pub trait ServerRefMut<'target, Target, Codec>: ServerBase
where
    Self: Sized,
{
    /// Creates a new server instance for the target object.
    fn new(target: &'target mut Target, request_buffer: usize) -> (Self, Self::Client);

    /// Serves the target object.
    ///
    /// Serving ends when the client is dropped.
    fn serve(self) -> impl Future<Output = Result<(), ServeError>>;
}

/// A server of a remotable trait taking the target object by shared reference.
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
    fn serve(self, spawn: bool) -> impl Future<Output = Result<(), ServeError>>;
}

/// A server of a remotable trait taking the target object by shared mutable reference.
pub trait ServerSharedMut<Target, Codec>: ServerBase
where
    Self: Sized,
{
    /// Creates a new server instance for a shared mutable reference to the target object.
    fn new(target: Arc<LocalRwLock<Target>>, request_buffer: usize) -> (Self, Self::Client);

    /// Serves the target object.
    ///
    /// If `spawn` is true, remote calls taking a `&self` reference are executed
    /// in parallel by spawning a task per call.
    /// Remote calls taking a `&mut self` reference are serialized by obtaining a write lock.
    ///
    /// Serving ends when the client is dropped.
    fn serve(self, spawn: bool) -> impl Future<Output = Result<(), ServeError>>;
}

/// A receiver of requests made by the client of a remotable trait.
pub trait ReqReceiver<Codec>: ServerBase + Stream<Item = Result<Self::Req, mpsc::RecvError>>
where
    Self: Sized,
{
    /// Request enum type.
    type Req;

    /// Creates a new request receiver instance together with its associated client.
    fn new(request_buffer: usize) -> (Self, Self::Client);

    /// Receives the next request, i.e. method call, from the client.
    ///
    /// Handle the request by matching on the variants of the request enum.
    /// Then reply with the result on the oneshot sender provided in the
    /// `__reply_tx` field of each enum variant.
    fn recv(&mut self) -> impl Future<Output = Result<Option<Self::Req>, mpsc::RecvError>> + Send;

    /// Closes the receiver half of the request channel without dropping it.
    ///
    /// This allows to process outstanding requests while stopping the client
    /// from sending new requests.
    fn close(&mut self);
}

/// Determines what should happen on the server-side if receiving an RTC
/// request fails.
///
/// Client-side behavior is unaffected by this choice, and will always
/// result in a [`CallError`] if the RTC request fails for any reason.
#[derive(Debug, Default, Clone)]
pub enum OnReqReceiveError {
    /// Ignore the failure to receive the request and write a log message.
    #[default]
    Ignore,
    /// Send the receive error over the local MPSC channel.
    Send(tokio::sync::mpsc::Sender<mpsc::RecvError>),
    /// Fail the server, returning the receive error.
    Fail,
}

impl OnReqReceiveError {
    #[doc(hidden)]
    pub async fn handle(&self, err: mpsc::RecvError) -> Result<(), ServeError> {
        match self {
            Self::Ignore => {
                tracing::warn!(%err, "receiving RTC request failed");
                Ok(())
            }
            Self::Send(tx) => {
                let _ = tx.send(err).await;
                Ok(())
            }
            Self::Fail => Err(ServeError::ReqReceive(err)),
        }
    }
}

/// RTC serving failed.
#[derive(Debug)]
pub enum ServeError {
    /// Receiving a request from the client failed.
    ReqReceive(mpsc::RecvError),
    /// Sending a reply to the client failed,
    ReplySend(SendingErrorKind),
}

impl From<mpsc::RecvError> for ServeError {
    fn from(err: mpsc::RecvError) -> Self {
        Self::ReqReceive(err)
    }
}

impl<T> From<SendingError<T>> for ServeError {
    fn from(err: SendingError<T>) -> Self {
        Self::ReplySend(err.kind())
    }
}

impl From<SendingErrorKind> for ServeError {
    fn from(err: SendingErrorKind) -> Self {
        Self::ReplySend(err)
    }
}

impl fmt::Display for ServeError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Self::ReqReceive(err) => write!(f, "failed to receive RTC request: {err}"),
            Self::ReplySend(err) => write!(f, "failed to send reply to RTC request: {err}"),
        }
    }
}

impl Error for ServeError {}

// Re-exports for proc macro usage.
#[doc(hidden)]
pub use crate::exec::task::spawn;
#[doc(hidden)]
pub use serde::{Deserialize, Serialize};
#[doc(hidden)]
pub use tokio::select;
#[doc(hidden)]
pub use tokio::sync::RwLock as LocalRwLock;
#[doc(hidden)]
pub use tokio::sync::broadcast as local_broadcast;
#[doc(hidden)]
pub use tokio::sync::mpsc as local_mpsc;
#[doc(hidden)]
pub type ReplyErrorSender = tokio::sync::mpsc::Sender<SendingErrorKind>;
#[doc(hidden)]
pub use futures::stream::Stream;
#[doc(hidden)]
pub use futures::stream::StreamExt;
#[doc(hidden)]
pub use tracing::Instrument;

/// Create channel for queueing reply sending errors.
#[doc(hidden)]
pub fn reply_error_channel() -> (ReplyErrorSender, tokio::sync::mpsc::Receiver<SendingErrorKind>) {
    tokio::sync::mpsc::channel(16)
}

/// Broadcast sender with no subscribers.
#[doc(hidden)]
pub fn empty_client_drop_tx() -> local_broadcast::Sender<()> {
    local_broadcast::channel(1).0
}

/// Missing maximum reply size value for backwards compatibility.
#[doc(hidden)]
pub const fn missing_max_reply_size() -> usize {
    usize::MAX
}

/// Send reply to request.
#[doc(hidden)]
pub async fn send_reply<T, Codec>(reply_tx: oneshot::Sender<T, Codec>, err_tx: &ReplyErrorSender, result: T)
where
    T: RemoteSend,
    Codec: codec::Codec,
{
    let Ok(sending) = reply_tx.send(result) else { return };

    let err_tx = err_tx.clone();
    exec::spawn(
        async move {
            if let Err(err) = sending.await {
                let kind = err.kind();
                match &kind {
                    SendingErrorKind::Send(base::SendErrorKind::Send(_)) => return,
                    SendingErrorKind::Dropped => return,
                    _ => (),
                }
                let _ = err_tx.send(kind).await;
            }
        }
        .in_current_span(),
    );
}

/// Serialization for `max_reply_size` field.
#[doc(hidden)]
pub mod serde_max_reply_size {
    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    /// Serialization function.
    pub fn serialize<S>(max_reply_size: &usize, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let max_reply_size = u64::try_from(*max_reply_size).unwrap_or(u64::MAX);
        max_reply_size.serialize(serializer)
    }

    /// Deserialization function.
    pub fn deserialize<'de, D>(deserializer: D) -> Result<usize, D::Error>
    where
        D: Deserializer<'de>,
    {
        let max_reply_size = u64::deserialize(deserializer)?;
        Ok(usize::try_from(max_reply_size).unwrap_or(usize::MAX))
    }
}
