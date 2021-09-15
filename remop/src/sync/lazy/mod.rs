//! Lazy transmission of values.

use serde::{Deserialize, Serialize};
use std::{error::Error, fmt, ops::Deref};
use tokio::sync::Mutex;

use super::{oneshot, remote, RemoteSend};
use crate::{chmux, codec::CodecT};

/// An error occured during receiving a lazily transmitted value.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum LazyError {
    /// Provider dropped before getting the value.
    Dropped,
    /// Receiving from a remote endpoint failed.
    RemoteReceive(remote::RecvError),
    /// Connecting a sent channel failed.
    RemoteConnect(chmux::ConnectError),
    /// Listening for a connection from a received channel failed.
    RemoteListen(chmux::ListenerError),
}

impl fmt::Display for LazyError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Self::Dropped => write!(f, "lazy provider dropped"),
            Self::RemoteReceive(err) => write!(f, "receive error: {}", err),
            Self::RemoteConnect(err) => write!(f, "connect error: {}", err),
            Self::RemoteListen(err) => write!(f, "listen error: {}", err),
        }
    }
}

impl From<oneshot::RecvError> for LazyError {
    fn from(err: oneshot::RecvError) -> Self {
        match err {
            oneshot::RecvError::Closed => Self::Dropped,
            oneshot::RecvError::RemoteReceive(err) => Self::RemoteReceive(err),
            oneshot::RecvError::RemoteConnect(err) => Self::RemoteConnect(err),
            oneshot::RecvError::RemoteListen(err) => Self::RemoteListen(err),
        }
    }
}

impl Error for LazyError {}

/// Lazy provider.
///
/// Stores a value and sends it to the [lazy consumer](Lazy)
/// when it requests the value.
///
/// If the lazy provider is dropped, the stored value is dropped and
/// the lazy consumer cannot request it anymore.
pub struct Provider {
    keep_tx: Option<tokio::sync::oneshot::Sender<()>>,
}

impl fmt::Debug for Provider {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Provider").finish_non_exhaustive()
    }
}

impl Provider {
    /// Keeps the provider alive until the value is requested
    /// or not required anymore.
    pub fn keep(mut self) {
        let _ = self.keep_tx.take().unwrap().send(());
    }

    /// Waits until the lazy provider can be safely dropped.
    ///
    /// This is the case when the value is requested by the [lazy consumer](Lazy)
    /// or the consumer is dropped.
    pub async fn done(&mut self) {
        self.keep_tx.as_mut().unwrap().closed().await
    }
}

impl Drop for Provider {
    fn drop(&mut self) {
        // empty
    }
}

/// Lazy consumer.
///
/// Allow the reception of a value when requested.
pub struct Lazy<T, Codec> {
    value: tokio::sync::RwLock<Option<Result<T, LazyError>>>,
    request_tx: Mutex<Option<oneshot::Sender<(), Codec>>>,
    value_rx: Mutex<Option<oneshot::Receiver<T, Codec>>>,
}

impl<T, Codec> fmt::Debug for Lazy<T, Codec> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Lazy").finish_non_exhaustive()
    }
}

impl<T, Codec> Lazy<T, Codec>
where
    T: RemoteSend,
    Codec: CodecT,
{
    /// Creates a new lazy consumer that will receive the specified value.
    ///
    /// The value is stored locally until the lazy consumer requests it
    /// or is dropped.
    pub fn new(value: T) -> Self {
        let (lazy, provider) = Self::provided(value);
        provider.keep();
        lazy
    }

    /// Creates a new pair of lazy consumer and provider with the specified value.
    pub fn provided(value: T) -> (Self, Provider) {
        let (value_tx, value_rx) = oneshot::channel();
        let (request_tx, request_rx) = oneshot::channel();
        let (keep_tx, keep_rx) = tokio::sync::oneshot::channel();

        tokio::spawn(async move {
            tokio::select! {
                biased;

                Err(_) = keep_rx => (),

                Ok(()) = request_rx => {
                    let _ = value_tx.send(value);
                }
            }
        });

        let provider = Provider { keep_tx: Some(keep_tx) };
        let lazy = Lazy {
            value: tokio::sync::RwLock::new(None),
            request_tx: Mutex::new(Some(request_tx)),
            value_rx: Mutex::new(Some(value_rx)),
        };

        (lazy, provider)
    }

    /// Fetches and caches the value from the provider.
    async fn fetch(&self) {
        if self.value.read().await.is_some() {
            return;
        }

        let mut value = self.value.write().await;
        if value.is_some() {
            return;
        }

        if let Some(request_tx) = self.request_tx.lock().await.take() {
            let _ = request_tx.send(());
        }

        let mut value_rx_opt = self.value_rx.lock().await;
        if let Some(value_rx) = &mut *value_rx_opt {
            *value = Some(value_rx.await.map_err(|err| err.into()));
            *value_rx_opt = None;
        }
    }

    /// Requests the value and returns a reference to it.
    ///
    /// The value is stored locally once received and subsequent
    /// invocations of this function will return a reference to
    /// the local copy.
    pub async fn get(&self) -> Result<Ref<'_, T>, LazyError> {
        self.fetch().await;

        let guard = self.value.read().await;
        if let Err(err) = guard.as_ref().unwrap() {
            return Err(err.clone());
        }

        let value_guard = tokio::sync::RwLockReadGuard::map(guard, |o| o.as_ref().unwrap().as_ref().unwrap());
        Ok(Ref(value_guard))
    }

    /// Consumes this object and returns the value.
    pub async fn into_inner(self) -> Result<T, LazyError> {
        self.fetch().await;
        self.value.into_inner().unwrap()
    }
}

/// A reference to a lazily received value.
pub struct Ref<'a, T>(tokio::sync::RwLockReadGuard<'a, T>);

impl<'a, T> fmt::Debug for Ref<'a, T>
where
    T: fmt::Debug,
{
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{:?}", &*self)
    }
}

impl<'a, T> Deref for Ref<'a, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &*self.0
    }
}
