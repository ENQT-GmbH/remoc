use bytes::Buf;
use futures::{future, task::noop_waker, FutureExt};
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
        RemoteSendError, BACKCHANNEL_MSG_ERROR,
    },
    receiver::RecvError,
    Receiver, Ref, ERROR_QUEUE,
};
use crate::{
    chmux,
    codec::{self},
    RemoteSend,
};

/// An error occurred during sending over an mpsc channel.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum SendError {
    /// The remote end closed the channel.
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
    /// True, if all receivers have been dropped.
    pub fn is_closed(&self) -> bool {
        matches!(self, Self::Closed)
    }
}

impl fmt::Display for SendError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Self::Closed => write!(f, "channel is closed"),
            Self::RemoteSend(err) => write!(f, "send error: {}", err),
            Self::RemoteConnect(err) => write!(f, "connect error: {}", err),
            Self::RemoteListen(err) => write!(f, "listen error: {}", err),
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
        f.debug_struct("Sender").finish_non_exhaustive()
    }
}

pub(crate) struct SenderInner<T, Codec> {
    tx: tokio::sync::watch::Sender<Result<T, RecvError>>,
    remote_send_err_tx: tokio::sync::mpsc::Sender<RemoteSendError>,
    remote_send_err_rx: Mutex<tokio::sync::mpsc::Receiver<RemoteSendError>>,
    current_err: Mutex<Option<RemoteSendError>>,
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
}

impl<T, Codec> Sender<T, Codec>
where
    T: Send + 'static,
{
    /// Creates a new sender.
    pub(crate) fn new(
        tx: tokio::sync::watch::Sender<Result<T, RecvError>>,
        remote_send_err_tx: tokio::sync::mpsc::Sender<RemoteSendError>,
        remote_send_err_rx: tokio::sync::mpsc::Receiver<RemoteSendError>,
    ) -> Self {
        let inner = SenderInner {
            tx,
            remote_send_err_tx,
            remote_send_err_rx: Mutex::new(remote_send_err_rx),
            current_err: Mutex::new(None),
            _codec: PhantomData,
        };
        Self { inner: Some(inner), successor_tx: Mutex::new(None) }
    }

    /// Sends a value over this channel.
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

    /// Returns a reference to the most recently sent value.
    #[inline]
    pub fn borrow(&self) -> Result<Ref<'_, T>, RecvError> {
        let ref_res = self.inner.as_ref().unwrap().tx.borrow();
        match &*ref_res {
            Ok(_) => Ok(Ref(ref_res)),
            Err(err) => Err(err.clone()),
        }
    }

    /// Completes when all receivers have been dropped.
    #[inline]
    pub async fn closed(&self) {
        self.inner.as_ref().unwrap().tx.closed().await
    }

    /// Returns whether all receivers have been dropped.
    #[inline]
    pub fn is_closed(&self) -> bool {
        self.inner.as_ref().unwrap().tx.is_closed()
    }

    /// Creates a new receiver subscribed to this sender.
    pub fn subscribe(&self) -> Receiver<T, Codec> {
        let inner = self.inner.as_ref().unwrap();
        Receiver::new(inner.tx.subscribe(), inner.remote_send_err_tx.clone())
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
}

impl<T, Codec> Drop for Sender<T, Codec> {
    fn drop(&mut self) {
        if let Some(succesor_tx) = self.successor_tx.lock().unwrap().take() {
            let _ = succesor_tx.send(self.inner.take().unwrap());
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
        // Prepare channel for takeover.
        let (successor_tx, successor_rx) = tokio::sync::oneshot::channel();
        *self.successor_tx.lock().unwrap() = Some(successor_tx);

        let port = PortSerializer::connect(|connect| {
            async move {
                // Sender has been dropped after sending, so we receive its channels.
                let SenderInner { tx, remote_send_err_rx, current_err, .. } = match successor_rx.await {
                    Ok(inner) => inner,
                    Err(_) => return,
                };
                let mut remote_send_err_rx = remote_send_err_rx.into_inner().unwrap();
                let mut current_err = current_err.into_inner().unwrap();

                // Establish chmux channel.
                let (mut raw_tx, raw_rx) = match connect.await {
                    Ok(tx_rx) => tx_rx,
                    Err(err) => {
                        let _ = tx.send(Err(RecvError::RemoteConnect(err)));
                        return;
                    }
                };

                // Decode raw received data using remote receiver.
                let mut remote_rx = base::Receiver::<Result<T, RecvError>, Codec>::new(raw_rx);

                // Process events.
                loop {
                    tokio::select! {
                        biased;

                        // Channel closure requested locally.
                        () = tx.closed() => break,

                        // Notify remote endpoint of error.
                        Some(_) = remote_send_err_rx.recv() => {
                            let _ = raw_tx.send(vec![BACKCHANNEL_MSG_ERROR].into()).await;
                        }
                        () = future::ready(()), if current_err.is_some() => {
                            let _ = raw_tx.send(vec![BACKCHANNEL_MSG_ERROR].into()).await;
                            current_err = None;
                        }

                        // Data received from remote endpoint.
                        res = remote_rx.recv() => {
                            let value = match res {
                                Ok(Some(value)) => value,
                                Ok(None) => break,
                                Err(err) => Err(RecvError::RemoteReceive(err)),
                            };
                            if tx.send(value).is_err() {
                                break;
                            }
                        }
                    }
                }
            }
            .boxed()
        })?;

        // Encode chmux port number in transport type and serialize it.
        let data = self.inner.as_ref().unwrap().tx.borrow().clone();
        let transported = TransportedSender::<T, Codec> { port, data, codec: PhantomData };
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
        let TransportedSender { port, data, .. } = TransportedSender::<T, Codec>::deserialize(deserializer)?;

        // Create internal communication channels.
        let (tx, mut rx) = tokio::sync::watch::channel(data);
        let (remote_send_err_tx, remote_send_err_rx) = tokio::sync::mpsc::channel(ERROR_QUEUE);
        let remote_send_err_tx2 = remote_send_err_tx.clone();

        // Accept chmux port request.
        PortDeserializer::accept(port, |local_port, request| {
            async move {
                // Accept chmux connection request.
                let (raw_tx, mut raw_rx) = match request.accept_from(local_port).await {
                    Ok(tx_rx) => tx_rx,
                    Err(err) => {
                        let _ = remote_send_err_tx.try_send(RemoteSendError::Listen(err));
                        return;
                    }
                };

                // Encode data using remote sender for sending.
                let mut remote_tx = base::Sender::<Result<T, RecvError>, Codec>::new(raw_tx);

                // Process events.
                loop {
                    tokio::select! {
                        biased;

                        // Backchannel message from remote endpoint.
                        backchannel_msg = raw_rx.recv() => {
                            match backchannel_msg {
                                Ok(Some(mut msg)) if msg.remaining() >= 1 => {
                                    if msg.get_u8() == BACKCHANNEL_MSG_ERROR {
                                        let _ = remote_send_err_tx.try_send(RemoteSendError::Forward);
                                    }
                                }
                                _ => break,
                            }
                        }

                        // Data to send to remote endpoint.
                        changed = rx.changed() => {
                            match changed {
                                Ok(()) => {
                                    let value = rx.borrow_and_update().clone();
                                    if let Err(err) = remote_tx.send(value).await {
                                        let _ = remote_send_err_tx.try_send(RemoteSendError::Send(err.kind));
                                    }
                                }
                                Err(_) => break,
                            }
                        }
                    }
                }
            }
            .boxed()
        })?;

        Ok(Self::new(tx, remote_send_err_tx2, remote_send_err_rx))
    }
}
