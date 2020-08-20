//! # Streaming remote procedure calls (RPC) over chmux.
//!
//! Provides remote method invocation using chmux transport.
//!
//! Use `[service]` macro to annotate your server trait.
//! A server method and a client proxy will be generated.
//!

#[cfg(feature = "socket")]
pub mod socket;

use futures::{
    sink::Sink,
    stream::{Stream, StreamExt},
    task::{Context, Poll},
};
use pin_project::{pin_project, pinned_drop};
use std::{error::Error, fmt, marker::PhantomData, pin::Pin};

use chmux::{ConnectError, ReceiveError, SendError};
use serde::{de::DeserializeOwned, Serialize};

pub use chrpc_macro::service;

/// An error occured during an RPC call.
#[derive(Debug)]
pub enum CallError {
    /// Connection has been rejected by server.
    Rejected,
    /// A multiplexer error has occured or it has been terminated.
    MultiplexerError,
    /// Error serializing the service request.
    SerializationError(Box<dyn Error + Send + 'static>),
    /// A deserialization error occured.
    DeserializationError(Box<dyn Error + Send + 'static>),
}

impl fmt::Display for CallError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Self::Rejected => write!(f, "Connection has been rejected by server."),
            Self::MultiplexerError => write!(f, "A multiplexer error has occured or it has been terminated."),
            Self::SerializationError(err) => write!(f, "A serialization error occured: {}", err),
            Self::DeserializationError(err) => write!(f, "A deserialization error occured: {}", err),
        }
    }
}

impl Error for CallError {}

impl From<ConnectError> for CallError {
    fn from(err: ConnectError) -> Self {
        match err {
            ConnectError::MultiplexerError => Self::MultiplexerError,
            ConnectError::Rejected => Self::Rejected,
            ConnectError::SerializationError(err) => Self::SerializationError(err),
        }
    }
}

impl From<ReceiveError> for CallError {
    fn from(err: ReceiveError) -> Self {
        match err {
            ReceiveError::MultiplexerError => Self::MultiplexerError,
            ReceiveError::DeserializationError(err) => Self::DeserializationError(err),
        }
    }
}

/// RPC sender.
#[pin_project(PinnedDrop)]
pub struct Sender<'a, T> {
    #[pin]
    sender: chmux::Sender<T>,
    _ghost: PhantomData<&'a ()>,
}

#[pinned_drop]
impl<'a, T> PinnedDrop for Sender<'a, T> {
    fn drop(self: Pin<&mut Self>) {
        // Drop implementation must be present to make reference guard work.
    }
}

impl<'a, T> Sink<T> for Sender<'a, T>
where
    T: Serialize,
{
    type Error = SendError;
    fn poll_ready(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Result<(), Self::Error>> {
        self.project().sender.poll_ready(cx)
    }
    fn start_send(self: Pin<&mut Self>, item: T) -> Result<(), Self::Error> {
        self.project().sender.start_send(item)
    }
    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Result<(), Self::Error>> {
        self.project().sender.poll_flush(cx)
    }
    fn poll_close(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Result<(), Self::Error>> {
        self.project().sender.poll_close(cx)
    }
}

impl<'a, T> Sender<'a, T>
where
    T: Serialize,
{
    #[doc(hidden)]
    pub fn new(sender: chmux::Sender<T>) -> Self {
        Self { sender, _ghost: PhantomData }
    }
}

/// RPC receiver.
#[pin_project(PinnedDrop)]
pub struct Receiver<'a, T> {
    #[pin]
    receiver: chmux::Receiver<T>,
    _ghost: PhantomData<&'a ()>,
}

#[pinned_drop]
impl<'a, T> PinnedDrop for Receiver<'a, T> {
    fn drop(self: Pin<&mut Self>) {
        // Drop implementation must be present to make reference guard work.
    }
}

impl<'a, T> Stream for Receiver<'a, T>
where
    T: DeserializeOwned,
{
    type Item = Result<T, ReceiveError>;
    fn poll_next(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Option<Self::Item>> {
        self.project().receiver.poll_next(cx)
    }
}

impl<'a, T> Receiver<'a, T>
where
    T: DeserializeOwned,
{
    #[doc(hidden)]
    pub fn new(receiver: chmux::Receiver<T>) -> Self {
        Self { receiver, _ghost: PhantomData }
    }

    /// Prevents the remote endpoint from sending new messages into this channel while
    /// allowing in-flight messages to be received.     
    pub async fn close(&mut self) {
        self.receiver.close().await
    }
}

/// A sink that sends items to an RPC method.
///
/// It can be closed, returning the value of the RPC method.
pub struct SendingCall<'a, S, T> {
    sender: Option<Pin<Box<chmux::Sender<S>>>>,
    receiver: Pin<Box<chmux::Receiver<T>>>,
    _ghost: PhantomData<&'a ()>,
}

impl<'a, S, T> Drop for SendingCall<'a, S, T> {
    fn drop(&mut self) {
        // Drop implementation must be present to make reference guard work.
    }
}

impl<'a, S, T> Sink<S> for SendingCall<'a, S, T>
where
    S: Serialize,
    T: DeserializeOwned,
{
    type Error = SendError;
    fn poll_ready(mut self: Pin<&mut Self>, cx: &mut Context) -> Poll<Result<(), Self::Error>> {
        self.sender.as_mut().unwrap().as_mut().poll_ready(cx)
    }
    fn start_send(mut self: Pin<&mut Self>, item: S) -> Result<(), Self::Error> {
        self.sender.as_mut().unwrap().as_mut().start_send(item)
    }
    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context) -> Poll<Result<(), Self::Error>> {
        self.sender.as_mut().unwrap().as_mut().poll_flush(cx)
    }
    fn poll_close(mut self: Pin<&mut Self>, cx: &mut Context) -> Poll<Result<(), Self::Error>> {
        self.sender.as_mut().unwrap().as_mut().poll_close(cx)
    }
}

impl<'a, S, T> SendingCall<'a, S, T>
where
    S: Serialize,
    T: DeserializeOwned,
{
    #[doc(hidden)]
    pub fn new(sender: chmux::Sender<S>, receiver: chmux::Receiver<T>) -> Self {
        Self { sender: Some(Box::pin(sender)), receiver: Box::pin(receiver), _ghost: PhantomData }
    }

    /// Closes the sink and returns a Future resolving to the return value of the
    /// RPC method.    
    pub async fn finish(mut self) -> Result<T, chmux::ReceiveError> {
        self.sender = None;
        match self.receiver.next().await {
            Some(ret) => ret,
            None => Err(chmux::ReceiveError::MultiplexerError),
        }
    }
}
