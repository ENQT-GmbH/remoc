use futures::Future;
use serde::{Deserialize, Serialize};
use std::fmt;

use super::{msg::RFnRequest, CallError};
use crate::{
    codec::CodecT,
    rch::{oneshot, RemoteSend},
};

/// Provides a remotely callable async FnOnce function.
///
/// Dropping the provider will stop making the function available for remote calls.
pub struct RFnOnceProvider {
    keep_tx: Option<tokio::sync::oneshot::Sender<()>>,
}

impl fmt::Debug for RFnOnceProvider {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("RFnOnceProvider").finish_non_exhaustive()
    }
}

impl RFnOnceProvider {
    /// Keeps the provider alive until it is not required anymore.
    pub fn keep(mut self) {
        let _ = self.keep_tx.take().unwrap().send(());
    }

    /// Waits until the provider can be safely dropped.
    ///
    /// This is the case when the [RFnOnce] is evaluated or dropped.
    pub async fn done(&mut self) {
        self.keep_tx.as_mut().unwrap().closed().await
    }
}

impl Drop for RFnOnceProvider {
    fn drop(&mut self) {
        // empty
    }
}

/// Calls an async FnOnce function possibly located on a remote endpoint.
#[derive(Serialize, Deserialize)]
#[serde(bound(serialize = "A: RemoteSend, R: RemoteSend, Codec: CodecT"))]
#[serde(bound(deserialize = "A: RemoteSend, R: RemoteSend, Codec: CodecT"))]
pub struct RFnOnce<A, R, Codec> {
    request_tx: oneshot::Sender<RFnRequest<A, R, Codec>, Codec>,
}

impl<A, R, Codec> fmt::Debug for RFnOnce<A, R, Codec> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("RFnOnce").finish_non_exhaustive()
    }
}

impl<A, R, Codec> RFnOnce<A, R, Codec>
where
    A: RemoteSend,
    R: RemoteSend,
    Codec: CodecT,
{
    /// Create a new remote function.
    pub fn new<F, Fut>(fun: F) -> Self
    where
        F: FnOnce(A) -> Fut + Send + 'static,
        Fut: Future<Output = R> + Send,
    {
        let (rfn, provider) = Self::provided(fun);
        provider.keep();
        rfn
    }

    /// Create a new remote function and return it with its provider.
    pub fn provided<F, Fut>(fun: F) -> (Self, RFnOnceProvider)
    where
        F: FnOnce(A) -> Fut + Send + 'static,
        Fut: Future<Output = R> + Send,
    {
        let (request_tx, request_rx) = oneshot::channel();
        let (keep_tx, keep_rx) = tokio::sync::oneshot::channel();

        tokio::spawn(async move {
            tokio::select! {
                biased;

                Err(_) = keep_rx => (),

                Ok(RFnRequest {argument, result_tx}) = request_rx => {
                    let result = fun(argument).await;
                    let _ = result_tx.send(result);
                }
            }
        });

        (Self { request_tx }, RFnOnceProvider { keep_tx: Some(keep_tx) })
    }

    /// Try to call the remote function.
    pub async fn try_call(self, argument: A) -> Result<R, CallError> {
        let (result_tx, result_rx) = oneshot::channel();
        let _ = self.request_tx.send(RFnRequest { argument, result_tx });

        let result = result_rx.await?;
        Ok(result)
    }
}

impl<A, RT, RE, Codec> RFnOnce<A, Result<RT, RE>, Codec>
where
    A: RemoteSend,
    RT: RemoteSend,
    RE: RemoteSend + From<CallError>,
    Codec: CodecT,
{
    /// Call the remote function.
    ///
    /// The [CallError] type must be convertable to the functions error type.
    pub async fn call(self, argument: A) -> Result<RT, RE> {
        Ok(self.try_call(argument).await??)
    }
}
