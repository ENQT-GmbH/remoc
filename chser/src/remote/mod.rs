use futures::{future::BoxFuture, ready, Future, FutureExt};
use std::task::{Context, Poll};

mod io;
mod receiver;
mod sender;

pub(crate) use receiver::{ObtainReceiverError, PortDeserializer, ReceiveError, Receiver};
pub(crate) use sender::{ObtainSenderError, PortSerializer, SendError, SendErrorKind, Sender};

const BIG_DATA_CHUNK_QUEUE: usize = 32;

/// Evaluates a future once and stores the result.
pub(crate) enum Obtainer<T, E> {
    Polling(BoxFuture<'static, Result<T, E>>),
    Ready(Result<T, E>),
}

impl<T, E> Obtainer<T, E>
where
    E: Clone,
{
    /// Creates a new obtainer that polls the specified future to obtain its value.
    pub fn future(fut: impl Future<Output = Result<T, E>> + Send + 'static) -> Self {
        Self::Polling(fut.boxed())
    }

    /// Creates a new obtainer with the specified value.
    pub fn ready(result: Result<T, E>) -> Self {
        Self::Ready(result)
    }

    /// Gets the (stored) result.
    pub async fn get(&mut self) -> Result<&mut T, E> {
        if let Self::Polling(fut) = self {
            *self = Self::Ready(fut.await);
        }

        if let Self::Ready(result) = self {
            result.as_mut().map_err(|err| err.clone())
        } else {
            unreachable!()
        }
    }

    /// Polls to get the (stored) result.
    pub fn poll_get(&mut self, cx: &mut Context) -> Poll<Result<&mut T, E>> {
        if let Self::Polling(fut) = self {
            *self = Self::Ready(ready!(fut.poll_unpin(cx)));
        }

        if let Self::Ready(result) = self {
            Poll::Ready(result.as_mut().map_err(|err| err.clone()))
        } else {
            unreachable!()
        }
    }
}
