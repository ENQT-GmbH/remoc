//! Remote channels.
//!
//! This module contains channels that can be used to exchange data of
//! arbitrary type with a remote endpoint.
//!
//! The data type must be serializable and deserializable by [serde] and
//! [sendable across thread boundaries](Send).
//! The [RemoteSend](crate::RemoteSend) trait is implemented automatically
//! for all types that fulfill these requirements.
//!
//! # Establishing a channel connection with a remote endpoint
//!
//! Except for a [base channel](base), a channel connection is established by sending
//! the Sender or Receiver half of a channel to a remote endpoint over an already
//! existing channel.
//! The transmitted channel-half can also be part of a larger object, such as a struct, tuple or enum.
//! Most channel types can even be forwarded over multiple connections.
//!
//! The primary purpose of a [base channel](base) is to provide an initial channel after
//! establishing a connection over a physical transport.
//! In most cases there is no need to create it directly and you will probably only use it
//! after it has been returned from an [initial connect function](crate::Connect).
//!
//! # Channel types
//!
//! [Broadcast](broadcast), [MPSC](mpsc), [oneshot] and [watch] channels are closely
//! modelled after the channels found in [tokio::sync].
//! They also work when both halves of the channel are local and can be forwarded
//! over multiple connections.
//!
//! An [local/remote channel](lr) is a more restricted version of an [MPSC](mpsc) channel.
//! It does not support forwarding and exactly one half of it must be on a remote endpoint.
//! Its benefit is, that it does not spawn an async task and thus uses less resources.
//! When in doubt use an [MPSC channel](mpsc) instead.
//!
//! A [binary channel](bin) can be used to exchange binary data over a channel.
//! It skips serialization and deserialization and thus is more efficient for binary data,
//! especially when using text codecs such as JSON.
//! It does support forwarding.
//! However, at least one half of it must be on a remote endpoint.
//!
//! # Acknowledgements and connection latency
//!
//! The channels do not wait for acknowledgement of transmitted values.
//! Thus, the next value can be queued for transmission before the previous value
//! has been received by the remote endpoint.
//! Hence, throughput is unaffected by the connection roundtrip time.
//!
//! A successful send does not guarantee that the transmitted value will be received
//! by the remote endpoint.
//! However, values cannot be lost intermittently, i.e. if a transmission error
//! occurs on the underlying physical connection the whole [chmux] connection will
//! be terminated eventually and sending will fail.
//!
//! If you need confirmation for every sent value, consider sending a [`oneshot::Sender`]`<()>`
//! along with it (for example as a tuple).
//! The remote endpoint can then send back an empty message as confirmation over the oneshot channel
//! after it has processed the received value.
//!
//! # Size considerations
//!
//! The size of the objects exchanged is not limited.
//! If the object is small enough it is first serialized into a buffer and then sent as
//! one message to the remote endpoint.
//! For larger objects, a serialization thread is spawned and the message is sent
//! chunk-by-chunk to avoid the need for a large temporary memory buffer.
//! Deserialization is also performed on-the-fly as data is received for large objects.
//!
//! Large values are always sent in chunks, so that one channel does not block other channels
//! for a long period of time.
//!
//! To avoid denial of service attacks when you exchange data with untrusted remote endpoints,
//! make sure that an object cannot grow to infinite size during deserialization.
//! For example, avoid using containers of unlimited size like [Vec] or [HashMap](std::collections::HashMap)
//! in your data structure or limit their maximum size during deserialization by applying
//! the `#[serde(deserialize_with="...")]` attribute to the fields containing them.
//!
//! # Static buffer size configuration for received channel-halves
//!
//! This is used to specify the local buffer size (in items) via a const generic type parameter
//! when the sender or receiver half of a channel *is received*.
//!
//! The default buffer size is [DEFAULT_BUFFER], which is currently 2 items.
//! It can be increased to improve performance, but this will also increase
//! the maximum amount of memory used per channel.
//! Setting the buffer size too high must be avoided, since this can lead to
//! the program running out of memory.
//!
//! # Error handling
//!
//! During sending it is often useful to treat errors that occur due to the disconnection, either graceful
//! or non-graceful, of the remote endpoint as an indication that the receiver is not
//! interested anymore in the transmitted data, rather than a hard error.
//! To avoid complicated error checks in your code, you can use [SendResultExt::into_disconnected]
//! to query whether a send error occurred due to the above cause and exit the sending process gracefully.
//!

use futures::FutureExt;
use serde::{Deserialize, Serialize};
use std::{
    error::Error,
    fmt,
    future::Future,
    pin::Pin,
    task::{Context, Poll, ready},
};

use crate::chmux;

mod interlock;

pub mod base;
pub mod bin;
pub mod broadcast;
pub mod lr;
pub mod mpsc;
pub mod oneshot;
pub mod watch;

/// Error connecting a remote channel.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ConnectError {
    /// The corresponding sender or receiver has been dropped.
    Dropped,
    /// Error initiating chmux connection.
    Connect(chmux::ConnectError),
    /// Error listening for or accepting chmux connection.
    Listen(chmux::ListenerError),
}

impl fmt::Display for ConnectError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Self::Dropped => write!(f, "other part was dropped"),
            Self::Connect(err) => write!(f, "connect error: {err}"),
            Self::Listen(err) => write!(f, "listen error: {err}"),
        }
    }
}

impl Error for ConnectError {}

/// Reason for closure of sender.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum ClosedReason {
    /// Channel was closed because receiver has been closed.
    Closed,
    /// Channel was closed because receiver has been dropped.
    Dropped,
    /// Channel was closed because connection between sender and receiver failed.
    Failed,
}

/// Back channel message that receiver has been closed.
pub(crate) const BACKCHANNEL_MSG_CLOSE: u8 = 0x01;

/// Back channel message that error has occurred.
pub(crate) const BACKCHANNEL_MSG_ERROR: u8 = 0x02;

/// Remote sending error.
#[derive(Clone)]
pub(crate) enum RemoteSendError {
    /// Send error.
    Send(base::SendErrorKind),
    /// Connect error.
    Connect(chmux::ConnectError),
    /// Listen for connect error.
    Listen(chmux::ListenerError),
    /// Forwarding error.
    Forward,
    /// Receiver was closed.
    Closed,
}

/// Common functions to query send errors for details.
///
/// This is implemented for all send errors in this module.
///
/// Since [watch] channels have no method to close them,
/// [is_closed](Self::is_closed) and [is_disconnected](Self::is_disconnected) are
/// equivalent for them.
pub trait SendErrorExt {
    /// Whether the remote endpoint closed the channel.
    fn is_closed(&self) -> bool;

    /// Whether the remote endpoint closed the channel, was dropped or the connection failed.
    fn is_disconnected(&self) -> bool;

    /// Whether the error is final, i.e. no further send operation can succeed.
    fn is_final(&self) -> bool;

    /// Whether the error is caused by the item to be sent.
    fn is_item_specific(&self) -> bool;
}

/// Common functions to query results of send operations for details.
///
/// This is implemented by all results from send operations in this module.
pub trait SendResultExt {
    /// The error type of this result.
    type Err: SendErrorExt;

    /// Whether the remote endpoint closed the channel.
    ///
    /// Returns `Ok(false)` if the send was successful and `Ok(true)` if the send failed for the
    /// above reason.
    /// The original error is returned if the failure had another cause.
    ///
    /// # Example
    ///
    /// In the following example the client counts upwards and the server closes the connections once
    /// it receives the number 5.
    /// The client stops counting as soon as sending fails.
    /// Note that the panic error path in the client would only be triggered by another error,
    /// such as a serialization error.
    ///
    /// ```
    /// use remoc::prelude::*;
    ///
    /// // This would be run on the client.
    /// async fn client(mut tx: rch::base::Sender<u32>) {
    ///     let mut n = 0;
    ///
    ///     loop {
    ///         if tx.send(n).await.into_closed().unwrap() {
    ///             break;
    ///         }
    ///         n += 1;
    ///     }
    ///
    ///     assert!(n > 5);
    /// }
    ///
    /// // This would be run on the server.
    /// async fn server(mut rx: rch::base::Receiver<u32>) {
    ///     while let Some(n) = rx.recv().await.unwrap() {
    ///         if n == 5 {
    ///             rx.close().await;
    ///         }
    ///     }
    /// }
    /// # tokio_test::block_on(remoc::doctest::client_server(client, server));
    /// ```
    fn into_closed(self) -> Result<bool, Self::Err>;

    /// Whether the remote endpoint closed the channel, was dropped or the connection failed.
    ///
    /// Returns `Ok(false)` if the send was successful and `Ok(true)` if the send failed for the
    /// above reasons.
    /// The original error is returned if the failure had another cause.
    ///
    /// # Example
    ///
    /// In the following example the client counts upwards and the server drops the connections once
    /// it receives the number 5.
    /// The client stops counting as soon as sending fails.
    /// Note that the panic error path in the client would only be triggered by another error,
    /// such as a serialization error.
    ///
    /// ```
    /// use remoc::prelude::*;
    ///
    /// // This would be run on the client.
    /// async fn client(mut tx: rch::base::Sender<u32>) {
    ///     let mut n = 0;
    ///
    ///     loop {
    ///         if tx.send(n).await.into_disconnected().unwrap() {
    ///             break;
    ///         }
    ///         n += 1;
    ///     }
    ///
    ///     assert!(n > 5);
    /// }
    ///
    /// // This would be run on the server.
    /// async fn server(mut rx: rch::base::Receiver<u32>) {
    ///     while let Some(n) = rx.recv().await.unwrap() {
    ///         if n == 5 {
    ///             break;
    ///         }
    ///     }
    /// }
    /// # tokio_test::block_on(remoc::doctest::client_server(client, server));
    /// ```
    fn into_disconnected(self) -> Result<bool, Self::Err>;
}

impl<E> SendResultExt for Result<(), E>
where
    E: SendErrorExt,
{
    type Err = E;

    fn into_closed(self) -> Result<bool, E> {
        match self {
            Ok(()) => Ok(false),
            Err(err) => {
                if err.is_closed() {
                    Ok(true)
                } else {
                    Err(err)
                }
            }
        }
    }

    fn into_disconnected(self) -> Result<bool, E> {
        match self {
            Ok(()) => Ok(false),
            Err(err) => {
                if err.is_disconnected() {
                    Ok(true)
                } else {
                    Err(err)
                }
            }
        }
    }
}

impl<T, E> SendResultExt for Result<Sending<T>, E>
where
    E: SendErrorExt,
{
    type Err = E;

    fn into_closed(self) -> Result<bool, E> {
        self.map(|_| ()).into_closed()
    }

    fn into_disconnected(self) -> Result<bool, E> {
        self.map(|_| ()).into_disconnected()
    }
}

/// Default buffer size in items, when the sender or receiver half of a channel *is received*.
///
/// This can be changed via a const generic type parameter of a sender or receiver.
///
/// The current default buffer size is 2.
pub const DEFAULT_BUFFER: usize = 2;

/// Default maximum allowed item size for sending and receiving items over remote channels.
///
/// The current default maximum allowed item size is 16 MB.
pub const DEFAULT_MAX_ITEM_SIZE: usize = 16_777_216;

/// Reason for why a value queued for sending failed to send.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SendingErrorKind {
    /// Sending failed.
    Send(base::SendErrorKind),
    /// The value was dropped while it was in the send queue or
    /// the result of the sending operation has already been read
    /// using [`Sending::try_result`].    
    Dropped,
}

impl fmt::Display for SendingErrorKind {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Self::Send(err) => write!(f, "{err}"),
            Self::Dropped => write!(f, "dropped"),
        }
    }
}

/// A value queued for sending failed to send.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SendingError<T> {
    /// Sending failed.
    Send(base::SendError<T>),
    /// The value was dropped while it was in the send queue or
    /// the result of the sending operation has already been read
    /// using [`Sending::try_result`].
    ///
    /// This can happen when the channel is closed before the value
    /// is sent.
    Dropped,
}

impl<T> fmt::Display for SendingError<T> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Self::Send(err) => write!(f, "{err}"),
            Self::Dropped => write!(f, "dropped"),
        }
    }
}

impl<T> Error for SendingError<T> where T: fmt::Debug {}

impl<T> SendingError<T> {
    /// Error kind.
    pub fn kind(&self) -> SendingErrorKind {
        match self {
            Self::Send(err) => SendingErrorKind::Send(err.kind.clone()),
            Self::Dropped => SendingErrorKind::Dropped,
        }
    }
}

/// Handle to obtain the result of a queued send operation.
///
/// Await this handle to obtain the result of the sending operation.
/// This is optional and only necessary if you want to explicitly handle errors
/// that can occur during sending; for example serialization errors or exceedence
/// of maximum item size.
///
/// You *should not* delay sending other items by awaiting this handle.
/// This would massively impact the throughput of the channel.
///
/// Dropping the handle *does not* abort sending the value.
pub struct Sending<T>(tokio::sync::oneshot::Receiver<Result<(), base::SendError<T>>>);

impl<T> fmt::Debug for Sending<T> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_tuple("Sending").finish()
    }
}

impl<T> Sending<T> {
    /// Tries to obtain the result of the sending operation.
    ///
    /// If the value is still queued for sending `None` is returned.
    pub fn try_result(&mut self) -> Option<Result<(), SendingError<T>>> {
        match self.0.try_recv() {
            Ok(Ok(())) => Some(Ok(())),
            Ok(Err(err)) => Some(Err(SendingError::Send(err))),
            Err(tokio::sync::oneshot::error::TryRecvError::Empty) => None,
            Err(tokio::sync::oneshot::error::TryRecvError::Closed) => Some(Err(SendingError::Dropped)),
        }
    }
}

impl<T> Future for Sending<T> {
    type Output = Result<(), SendingError<T>>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
        let res = match ready!(self.0.poll_unpin(cx)) {
            Ok(Ok(())) => Ok(()),
            Ok(Err(err)) => Err(SendingError::Send(err)),
            Err(_) => Err(SendingError::Dropped),
        };
        Poll::Ready(res)
    }
}

impl<T> Drop for Sending<T> {
    fn drop(&mut self) {
        if let Ok(Err(err)) = self.0.try_recv() {
            tracing::warn!(%err, "sending over remote channel failed");
        }
    }
}
