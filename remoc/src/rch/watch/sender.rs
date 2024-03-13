use futures::{task::noop_waker, FutureExt};
use serde::{Deserialize, Serialize};
use std::{
    error::Error,
    fmt,
    marker::PhantomData,
    sync::Mutex,
    task::{Context, Poll},
};

use super::{
    super::{
        base::{self, PortDeserializer, PortSerializer},
        RemoteSendError, SendErrorExt,
    },
    receiver::RecvError,
    Receiver, Ref, ERROR_QUEUE,
};
use crate::{chmux, codec, RemoteSend};

/// An error occurred during sending over an mpsc channel.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum SendError {
    /// The receiver was dropped or the connection failed.
    Closed,
    /// Sending to a remote endpoint failed.
    RemoteSend(base::SendErrorKind),
    /// Connecting a sent channel failed.
    RemoteConnect(chmux::ConnectError),
    /// Listening to a received channel failed.
    RemoteListen(chmux::ListenerError),
    /// Forwarding at a remote endpoint to another remote endpoint failed.
    RemoteForward,
}

impl SendError {
    /// True, if the remote endpoint was dropped or the connection failed.
    pub fn is_closed(&self) -> bool {
        !matches!(self, Self::RemoteSend(base::SendErrorKind::Serialize(_)))
    }

    /// True, if the remote endpoint was dropped or the connection failed.
    pub fn is_disconnected(&self) -> bool {
        !matches!(self, Self::RemoteSend(base::SendErrorKind::Serialize(_)))
    }

    /// Returns whether the error is final, i.e. no further send operation can succeed.
    pub fn is_final(&self) -> bool {
        match self {
            Self::RemoteSend(err) => err.is_final(),
            Self::Closed | Self::RemoteConnect(_) | Self::RemoteListen(_) | Self::RemoteForward => true,
        }
    }
}

impl SendErrorExt for SendError {
    fn is_closed(&self) -> bool {
        self.is_closed()
    }

    fn is_disconnected(&self) -> bool {
        self.is_disconnected()
    }

    fn is_final(&self) -> bool {
        self.is_final()
    }
}

impl fmt::Display for SendError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Self::Closed => write!(f, "channel is closed"),
            Self::RemoteSend(err) => write!(f, "send error: {err}"),
            Self::RemoteConnect(err) => write!(f, "connect error: {err}"),
            Self::RemoteListen(err) => write!(f, "listen error: {err}"),
            Self::RemoteForward => write!(f, "forwarding error"),
        }
    }
}

impl Error for SendError {}

impl From<RemoteSendError> for SendError {
    fn from(err: RemoteSendError) -> Self {
        match err {
            RemoteSendError::Send(err) => Self::RemoteSend(err),
            RemoteSendError::Connect(err) => Self::RemoteConnect(err),
            RemoteSendError::Listen(err) => Self::RemoteListen(err),
            RemoteSendError::Forward => Self::RemoteForward,
            RemoteSendError::Closed => Self::Closed,
        }
    }
}

/// Send values to the associated [Receiver](super::Receiver), which may be located on a remote endpoint.
///
/// Instances are created by the [channel](super::channel) function.
pub struct Sender<T, Codec = codec::Default> {
    inner: Option<SenderInner<T, Codec>>,
    successor_tx: Mutex<Option<tokio::sync::oneshot::Sender<SenderInner<T, Codec>>>>,
}

impl<T, Codec> fmt::Debug for Sender<T, Codec> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("Sender").finish()
    }
}

pub(crate) struct SenderInner<T, Codec> {
    tx: tokio::sync::watch::Sender<Result<T, RecvError>>,
    remote_send_err_tx: tokio::sync::mpsc::Sender<RemoteSendError>,
    remote_send_err_rx: Mutex<tokio::sync::mpsc::Receiver<RemoteSendError>>,
    current_err: Mutex<Option<RemoteSendError>>,
    max_item_size: usize,
    _codec: PhantomData<Codec>,
}

/// Watch sender in transport.
#[derive(Serialize, Deserialize)]
pub(crate) struct TransportedSender<T, Codec> {
    /// chmux port number.
    port: u32,
    /// Current data value.
    data: Result<T, RecvError>,
    /// Data codec.
    codec: PhantomData<Codec>,
    /// Maximum item size in bytes.
    #[serde(default = "default_max_item_size")]
    max_item_size: u64,
}

const fn default_max_item_size() -> u64 {
    u64::MAX
}

impl<T, Codec> Sender<T, Codec>
where
    T: Send + 'static,
{
    /// Creates a new sender.
    pub(crate) fn new(
        tx: tokio::sync::watch::Sender<Result<T, RecvError>>,
        remote_send_err_tx: tokio::sync::mpsc::Sender<RemoteSendError>,
        remote_send_err_rx: tokio::sync::mpsc::Receiver<RemoteSendError>, max_item_size: usize,
    ) -> Self {
        let inner = SenderInner {
            tx,
            remote_send_err_tx,
            remote_send_err_rx: Mutex::new(remote_send_err_rx),
            current_err: Mutex::new(None),
            max_item_size,
            _codec: PhantomData,
        };
        Self { inner: Some(inner), successor_tx: Mutex::new(None) }
    }

    /// Sends a value over this channel, notifying all receivers.
    ///
    /// This method fails if all receivers have been dropped or become disconnected.
    ///
    /// # Error reporting
    /// Sending and error reporting are done asynchronously.
    /// Thus, the reporting of an error may be delayed and this function may
    /// return errors caused by previous invocations.
    #[inline]
    pub fn send(&self, value: T) -> Result<(), SendError> {
        match self.inner.as_ref().unwrap().tx.send(Ok(value)) {
            Ok(()) => Ok(()),
            Err(_) => match self.error() {
                Some(err) => Err(err),
                None => Err(SendError::Closed),
            },
        }
    }

    /// Modifies the watched value and notifies all receivers.
    ///
    /// This method never fails, even if all receivers have been dropped or become
    /// disconnected.
    ///
    /// # Panics
    /// This method panics if calling `func` results in a panic.
    #[inline]
    pub fn send_modify<F>(&self, func: F)
    where
        F: FnOnce(&mut T),
    {
        self.inner.as_ref().unwrap().tx.send_modify(move |v| func(v.as_mut().unwrap()))
    }

    /// Sends a new value via the channel, notifying all receivers and returning the
    /// previous value in the channel.
    ///
    /// This method never fails, even if all receivers have been dropped or become
    /// disconnected.
    #[inline]
    pub fn send_replace(&self, value: T) -> T {
        self.inner.as_ref().unwrap().tx.send_replace(Ok(value)).unwrap()
    }

    /// Returns a reference to the most recently sent value.
    #[inline]
    pub fn borrow(&self) -> Ref<'_, T> {
        Ref(self.inner.as_ref().unwrap().tx.borrow())
    }

    /// Completes when all receivers have been dropped or the connection failed.
    #[inline]
    pub async fn closed(&self) {
        self.inner.as_ref().unwrap().tx.closed().await
    }

    /// Returns whether all receivers have been dropped or the connection failed.
    #[inline]
    pub fn is_closed(&self) -> bool {
        self.inner.as_ref().unwrap().tx.is_closed()
    }

    /// Creates a new receiver subscribed to this sender.
    pub fn subscribe(&self) -> Receiver<T, Codec> {
        let inner = self.inner.as_ref().unwrap();
        Receiver::new(inner.tx.subscribe(), inner.remote_send_err_tx.clone(), None)
    }

    fn update_error(&self) {
        let inner = self.inner.as_ref().unwrap();
        let mut current_err = inner.current_err.lock().unwrap();
        if current_err.is_some() {
            return;
        }

        let mut remote_send_err_rx = inner.remote_send_err_rx.lock().unwrap();

        let waker = noop_waker();
        let mut cx = Context::from_waker(&waker);
        if let Poll::Ready(err_opt) = remote_send_err_rx.poll_recv(&mut cx) {
            *current_err = err_opt;
        }
    }

    /// Returns the error that occurred during sending to a remote endpoint, if any.
    ///
    /// # Error reporting
    /// Sending and error reporting are done asynchronously.
    /// Thus, the reporting of an error may be delayed.
    pub fn error(&self) -> Option<SendError> {
        self.update_error();

        let inner = self.inner.as_ref().unwrap();
        let current_err = inner.current_err.lock().unwrap();
        current_err.clone().map(|err| err.into())
    }

    /// Clears the error that occurred during sending to a remote endpoint.
    pub fn clear_error(&mut self) {
        self.update_error();

        let inner = self.inner.as_ref().unwrap();
        let mut current_err = inner.current_err.lock().unwrap();
        *current_err = None;
    }

    /// Maximum allowed item size in bytes.
    pub fn max_item_size(&self) -> usize {
        self.inner.as_ref().unwrap().max_item_size
    }

    /// Sets the maximum allowed item size in bytes.
    pub fn set_max_item_size(&mut self, max_item_size: usize) {
        self.inner.as_mut().unwrap().max_item_size = max_item_size;
    }
}

impl<T, Codec> Drop for Sender<T, Codec> {
    fn drop(&mut self) {
        if let Some(successor_tx) = self.successor_tx.lock().unwrap().take() {
            let _ = successor_tx.send(self.inner.take().unwrap());
        }
    }
}

impl<T, Codec> Serialize for Sender<T, Codec>
where
    T: RemoteSend + Sync + Clone,
    Codec: codec::Codec,
{
    /// Serializes this sender for sending over a chmux channel.
    #[inline]
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let max_item_size = self.max_item_size();

        // Prepare channel for takeover.
        let (successor_tx, successor_rx) = tokio::sync::oneshot::channel();
        *self.successor_tx.lock().unwrap() = Some(successor_tx);

        let port = PortSerializer::connect(move |connect| {
            async move {
                // Sender has been dropped after sending, so we receive its channels.
                let SenderInner { tx, remote_send_err_rx, current_err, .. } = match successor_rx.await {
                    Ok(inner) => inner,
                    Err(_) => return,
                };
                let remote_send_err_rx = remote_send_err_rx.into_inner().unwrap();
                let current_err = current_err.into_inner().unwrap();

                // Establish chmux channel.
                let (raw_tx, raw_rx) = match connect.await {
                    Ok(tx_rx) => tx_rx,
                    Err(err) => {
                        let _ = tx.send(Err(RecvError::RemoteConnect(err)));
                        return;
                    }
                };

                super::recv_impl::<T, Codec>(tx, raw_tx, raw_rx, remote_send_err_rx, current_err, max_item_size)
                    .await;
            }
            .boxed()
        })?;

        // Encode chmux port number in transport type and serialize it.
        let data = self.inner.as_ref().unwrap().tx.borrow().clone();
        let transported = TransportedSender::<T, Codec> {
            port,
            data,
            max_item_size: max_item_size.try_into().unwrap_or(u64::MAX),
            codec: PhantomData,
        };
        transported.serialize(serializer)
    }
}

impl<'de, T, Codec> Deserialize<'de> for Sender<T, Codec>
where
    T: RemoteSend + Sync + Clone,
    Codec: codec::Codec,
{
    /// Deserializes this sender after it has been received over a chmux channel.
    #[inline]
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        // Get chmux port number from deserialized transport type.
        let TransportedSender { port, data, max_item_size, .. } =
            TransportedSender::<T, Codec>::deserialize(deserializer)?;
        let max_item_size = usize::try_from(max_item_size).unwrap_or(usize::MAX);
        if data.is_err() {
            return Err(serde::de::Error::custom("received watch data with error"));
        }

        // Create internal communication channels.
        let (tx, rx) = tokio::sync::watch::channel(data);
        let (remote_send_err_tx, remote_send_err_rx) = tokio::sync::mpsc::channel(ERROR_QUEUE);
        let remote_send_err_tx2 = remote_send_err_tx.clone();

        // Accept chmux port request.
        PortDeserializer::accept(port, move |local_port, request| {
            async move {
                // Accept chmux connection request.
                let (raw_tx, raw_rx) = match request.accept_from(local_port).await {
                    Ok(tx_rx) => tx_rx,
                    Err(err) => {
                        let _ = remote_send_err_tx.try_send(RemoteSendError::Listen(err));
                        return;
                    }
                };

                super::send_impl::<T, Codec>(rx, raw_tx, raw_rx, remote_send_err_tx, max_item_size).await;
            }
            .boxed()
        })?;

        Ok(Self::new(tx, remote_send_err_tx2, remote_send_err_rx, max_item_size))
    }
}
