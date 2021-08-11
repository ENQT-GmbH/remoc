use futures::{Future, FutureExt, future::BoxFuture};



pub mod codec;
pub mod mpsc;
pub mod base_sender;
pub mod base_receiver;
pub mod remote_sender;
pub mod remote_receiver;

/// Evaluates a future once and stores the result.
enum Obtainer<T, E> {
    Obtaining(BoxFuture<'static, Result<T, E>>),
    Obtained(Result<T, E>),
}

impl<T, E> Obtainer<T, E>
where
    E: Clone,
{
    /// Creates a new obtainer from the specified future.
    pub fn new(fut: impl Future<Output = Result<T, E>> + Send + 'static) -> Self {
        Self::Obtaining(fut.boxed())
    }

    /// Gets the (stored) result of the future.
    pub async fn get(&mut self) -> Result<&mut T, E> {
        if let Self::Obtaining(fut) = self {
            *self = Self::Obtained(fut.await);
        }

        if let Self::Obtained(result) = self {
            result.as_mut().map_err(|err| err.clone())
        } else {
            unreachable!()
        }
    }
}
