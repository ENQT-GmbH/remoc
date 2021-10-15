use futures::{future, pin_mut, Future};
use serde::{Deserialize, Serialize};
use std::fmt;

use super::{msg::RFnRequest, CallError};
use crate::{
    codec::{self},
    rch::{buffer, mpsc, oneshot},
    RemoteSend,
};

/// Provides a remotely callable async FnMut function.
///
/// Dropping the provider will stop making the function available for remote calls.
pub struct RFnMutProvider {
    keep_tx: Option<tokio::sync::oneshot::Sender<()>>,
}

impl fmt::Debug for RFnMutProvider {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("RFnMutProvider").finish_non_exhaustive()
    }
}

impl RFnMutProvider {
    /// Keeps the provider alive until it is not required anymore.
    pub fn keep(mut self) {
        let _ = self.keep_tx.take().unwrap().send(());
    }

    /// Waits until the provider can be safely dropped.
    ///
    /// This is the case when the [RFnMut] is dropped.
    pub async fn done(&mut self) {
        self.keep_tx.as_mut().unwrap().closed().await
    }
}

impl Drop for RFnMutProvider {
    fn drop(&mut self) {
        // empty
    }
}

/// Calls an async FnMut function possibly located on a remote endpoint.
#[derive(Serialize, Deserialize)]
#[serde(bound(serialize = "A: RemoteSend, R: RemoteSend, Codec: codec::Codec"))]
#[serde(bound(deserialize = "A: RemoteSend, R: RemoteSend, Codec: codec::Codec"))]
pub struct RFnMut<A, R, Codec = codec::Default> {
    request_tx: mpsc::Sender<RFnRequest<A, R, Codec>, Codec, buffer::Custom<1>>,
}

impl<A, R, Codec> fmt::Debug for RFnMut<A, R, Codec> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("RFn").finish_non_exhaustive()
    }
}

impl<A, R, Codec> RFnMut<A, R, Codec>
where
    A: RemoteSend,
    R: RemoteSend,
    Codec: codec::Codec,
{
    /// Create a new remote function.
    pub fn new<F, Fut>(fun: F) -> Self
    where
        F: FnMut(A) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = R> + Send,
    {
        let (rfn, provider) = Self::provided(fun);
        provider.keep();
        rfn
    }

    /// Create a new remote function and return it with its provider.
    pub fn provided<F, Fut>(mut fun: F) -> (Self, RFnMutProvider)
    where
        F: FnMut(A) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = R> + Send,
    {
        let (request_tx, request_rx) = mpsc::channel(1);
        let request_tx = request_tx.set_buffer();
        let mut request_rx = request_rx.set_buffer::<buffer::Custom<1>>();
        let (keep_tx, keep_rx) = tokio::sync::oneshot::channel();

        tokio::spawn(async move {
            let term = async move {
                if let Ok(()) = keep_rx.await {
                    future::pending().await
                }
            };
            pin_mut!(term);

            loop {
                tokio::select! {
                    biased;

                    () = &mut term => break,

                    req_res = request_rx.recv() => {
                        match req_res {
                            Ok(Some(RFnRequest {argument, result_tx})) => {
                                let result = fun(argument).await;
                                let _ = result_tx.send(result);
                            }
                            Ok(None) => break,
                            Err(_) => (),
                        }
                    }
                }
            }
        });

        (Self { request_tx }, RFnMutProvider { keep_tx: Some(keep_tx) })
    }

    /// Try to call the remote function.
    pub async fn try_call(&mut self, argument: A) -> Result<R, CallError> {
        let (result_tx, result_rx) = oneshot::channel();
        let _ = self.request_tx.send(RFnRequest { argument, result_tx }).await;

        let result = result_rx.await?;
        Ok(result)
    }
}

impl<A, RT, RE, Codec> RFnMut<A, Result<RT, RE>, Codec>
where
    A: RemoteSend,
    RT: RemoteSend,
    RE: RemoteSend + From<CallError>,
    Codec: codec::Codec,
{
    /// Call the remote function.
    ///
    /// The [CallError] type must be convertable to the functions error type.
    pub async fn call(&mut self, argument: A) -> Result<RT, RE> {
        Ok(self.try_call(argument).await??)
    }
}

impl<A, R, Codec> Drop for RFnMut<A, R, Codec> {
    fn drop(&mut self) {
        // empty
    }
}
