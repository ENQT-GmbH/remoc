use bytes::Buf;
use futures::FutureExt;
use serde::{Deserialize, Serialize};
use std::{
    convert::TryFrom,
    error::Error,
    fmt,
    marker::PhantomData,
    sync::{Arc, Weak},
};

use super::{
    super::{
        remote::{self, PortDeserializer, PortSerializer},
        RemoteSend, RemoteSendError, BACKCHANNEL_MSG_CLOSE, BACKCHANNEL_MSG_ERROR,
    },
    receiver::RecvError,
};
use crate::{chmux, codec::CodecT};

/// An error occured during sending over an mpsc channel.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum SendError<T> {
    /// The remote end closed the channel.
    Closed(T),
    /// Sending to a remote endpoint failed.
    RemoteSend(remote::SendErrorKind),
    /// Connecting a sent channel failed.
    RemoteConnect(chmux::ConnectError),
    /// Listening for a received channel failed.
    RemoteListen(chmux::ListenerError),
    /// Forwarding at a remote endpoint to another remote endpoint failed.
    RemoteForward,
}

impl<T> SendError<T> {
    /// True, if the remote endpoint closed the channel.
    pub fn is_closed(&self) -> bool {
        matches!(self, Self::Closed(_))
    }
}

impl<T> fmt::Display for SendError<T> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Self::Closed(_) => write!(f, "channel is closed"),
            Self::RemoteSend(err) => write!(f, "send error: {}", err),
            Self::RemoteConnect(err) => write!(f, "connect error: {}", err),
            Self::RemoteListen(err) => write!(f, "listen error: {}", err),
            Self::RemoteForward => write!(f, "forwarding error"),
        }
    }
}

impl<T> Error for SendError<T> where T: fmt::Debug {}

impl<T> From<RemoteSendError> for SendError<T> {
    fn from(err: RemoteSendError) -> Self {
        match err {
            RemoteSendError::Send(err) => Self::RemoteSend(err),
            RemoteSendError::Connect(err) => Self::RemoteConnect(err),
            RemoteSendError::Listen(err) => Self::RemoteListen(err),
            RemoteSendError::Forward => Self::RemoteForward,
        }
    }
}

/// An error occured during trying to send over an mpsc channel.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum TrySendError<T> {
    /// The remote end closed the channel.
    Closed(T),
    /// The data could not be sent on the channel because the channel
    /// is currently full and sending would require blocking.
    Full(T),
    /// Sending to a remote endpoint failed.
    RemoteSend(remote::SendErrorKind),
    /// Connecting a sent channel failed.
    RemoteConnect(chmux::ConnectError),
    /// Listening for a received channel failed.
    RemoteListen(chmux::ListenerError),
    /// Forwarding at a remote endpoint to another remote endpoint failed.
    RemoteForward,
}

impl<T> fmt::Display for TrySendError<T> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Self::Closed(_) => write!(f, "channel is closed"),
            Self::Full(_) => write!(f, "channel is full"),
            Self::RemoteSend(err) => write!(f, "send error: {}", err),
            Self::RemoteConnect(err) => write!(f, "connect error: {}", err),
            Self::RemoteListen(err) => write!(f, "listen error: {}", err),
            Self::RemoteForward => write!(f, "forwarding error"),
        }
    }
}

impl<T> From<RemoteSendError> for TrySendError<T> {
    fn from(err: RemoteSendError) -> Self {
        match err {
            RemoteSendError::Send(err) => Self::RemoteSend(err),
            RemoteSendError::Connect(err) => Self::RemoteConnect(err),
            RemoteSendError::Listen(err) => Self::RemoteListen(err),
            RemoteSendError::Forward => Self::RemoteForward,
        }
    }
}

impl<T> From<SendError<T>> for TrySendError<T> {
    fn from(err: SendError<T>) -> Self {
        match err {
            SendError::Closed(v) => Self::Closed(v),
            SendError::RemoteSend(err) => Self::RemoteSend(err),
            SendError::RemoteConnect(err) => Self::RemoteConnect(err),
            SendError::RemoteListen(err) => Self::RemoteListen(err),
            SendError::RemoteForward => Self::RemoteForward,
        }
    }
}

impl<T> TryFrom<TrySendError<T>> for SendError<T> {
    type Error = TrySendError<T>;

    fn try_from(err: TrySendError<T>) -> Result<Self, Self::Error> {
        match err {
            TrySendError::Closed(v) => Ok(Self::Closed(v)),
            TrySendError::RemoteSend(err) => Ok(Self::RemoteSend(err)),
            TrySendError::RemoteConnect(err) => Ok(Self::RemoteConnect(err)),
            TrySendError::RemoteForward => Ok(Self::RemoteForward),
            other => Err(other),
        }
    }
}

impl<T> Error for TrySendError<T> where T: fmt::Debug {}

/// Send values to the associated [Receiver](super::Receiver), which may be located on a remote endpoint.
///
/// Instances are created by the [channel](super::channel) function.
pub struct Sender<T, Codec, const BUFFER: usize> {
    tx: Weak<tokio::sync::mpsc::Sender<Result<T, RecvError>>>,
    closed_rx: tokio::sync::watch::Receiver<bool>,
    remote_send_err_rx: tokio::sync::watch::Receiver<Option<RemoteSendError>>,
    _dropped_tx: tokio::sync::mpsc::Sender<()>,
    _codec: PhantomData<Codec>,
}

impl<T, Codec, const BUFFER: usize> fmt::Debug for Sender<T, Codec, BUFFER> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("Sender").finish_non_exhaustive()
    }
}

impl<T, Codec, const BUFFER: usize> Clone for Sender<T, Codec, BUFFER> {
    fn clone(&self) -> Self {
        Self {
            tx: self.tx.clone(),
            closed_rx: self.closed_rx.clone(),
            remote_send_err_rx: self.remote_send_err_rx.clone(),
            _dropped_tx: self._dropped_tx.clone(),
            _codec: PhantomData,
        }
    }
}

/// Mpsc sender in transport.
#[derive(Serialize, Deserialize)]
pub struct TransportedSender<T, Codec> {
    /// chmux port number. `None` if closed.
    port: Option<u32>,
    /// Data type.
    data: PhantomData<T>,
    /// Data codec.
    codec: PhantomData<Codec>,
}

impl<T, Codec, const BUFFER: usize> Sender<T, Codec, BUFFER>
where
    T: Send + 'static,
{
    /// Creates a new sender.
    pub(crate) fn new(
        tx: tokio::sync::mpsc::Sender<Result<T, RecvError>>, mut closed_rx: tokio::sync::watch::Receiver<bool>,
        remote_send_err_rx: tokio::sync::watch::Receiver<Option<RemoteSendError>>,
    ) -> Self {
        let tx = Arc::new(tx);
        let (dropped_tx, mut dropped_rx) = tokio::sync::mpsc::channel(1);

        let this = Self {
            tx: Arc::downgrade(&tx),
            closed_rx: closed_rx.clone(),
            remote_send_err_rx,
            _dropped_tx: dropped_tx,
            _codec: PhantomData,
        };

        // Drop strong reference to sender when channel is closed.
        tokio::spawn(async move {
            loop {
                tokio::select! {
                    _ = closed_rx.changed() => {
                        if *closed_rx.borrow() {
                            break;
                        }
                    },
                    _ = dropped_rx.recv() => break,
                }
            }

            drop(tx);
        });

        this
    }

    /// Creates a new sender that is closed.
    pub(crate) fn new_closed() -> Self {
        Self {
            tx: Weak::new(),
            closed_rx: tokio::sync::watch::channel(true).1,
            remote_send_err_rx: tokio::sync::watch::channel(None).1,
            _dropped_tx: tokio::sync::mpsc::channel(1).0,
            _codec: PhantomData,
        }
    }

    /// Sends a value over this channel.
    pub async fn send(&self, value: T) -> Result<(), SendError<T>> {
        if let Some(err) = self.remote_send_err_rx.borrow().as_ref() {
            return Err(err.clone().into());
        }

        if let Some(tx) = self.tx.upgrade() {
            if let Err(err) = tx.send(Ok(value)).await {
                return Err(SendError::Closed(err.0.expect("unreachable")));
            }
        } else {
            return Err(SendError::Closed(value));
        }

        Ok(())
    }

    /// Attempts to immediately send a message over this channel.
    pub fn try_send(&self, value: T) -> Result<(), TrySendError<T>> {
        if let Some(err) = self.remote_send_err_rx.borrow().as_ref() {
            return Err(err.clone().into());
        }

        match self.tx.upgrade() {
            Some(tx) => match tx.try_send(Ok(value)) {
                Ok(()) => Ok(()),
                Err(tokio::sync::mpsc::error::TrySendError::Full(err)) => {
                    Err(TrySendError::Full(err.expect("unreachable")))
                }
                Err(tokio::sync::mpsc::error::TrySendError::Closed(err)) => {
                    Err(TrySendError::Closed(err.expect("unreachable")))
                }
            },
            None => Err(TrySendError::Closed(value)),
        }
    }

    /// Wait for channel capacity, returning an owned permit.
    /// Once capacity to send one message is available, it is reserved for the caller.
    pub async fn reserve(&self) -> Result<Permit<T>, SendError<()>> {
        if let Some(err) = self.remote_send_err_rx.borrow().as_ref() {
            return Err(err.clone().into());
        }

        if let Some(tx) = self.tx.upgrade() {
            let tx = (*tx).clone();
            match tx.reserve_owned().await {
                Ok(permit) => Ok(Permit(permit)),
                Err(_) => Err(SendError::Closed(())),
            }
        } else {
            Err(SendError::Closed(()))
        }
    }

    /// Returns the current capacity of the channel.
    ///
    /// Zero is returned when the channel has been closed or an occur has occured.
    pub fn capacity(&self) -> usize {
        match self.tx.upgrade() {
            Some(tx) => tx.capacity(),
            None => 0,
        }
    }

    /// Completes when the receiver has been closed or dropped.
    pub async fn closed(&self) {
        let mut closed = self.closed_rx.clone();
        while !*closed.borrow() {
            if closed.changed().await.is_err() {
                break;
            }
        }
    }

    /// Returns whether the receiver has been closed or dropped.
    pub fn is_closed(&self) -> bool {
        *self.closed_rx.borrow()
    }

    /// Sets the codec that will be used when sending this sender to a remote endpoint.
    pub fn set_codec<NewCodec>(self) -> Sender<T, NewCodec, BUFFER> {
        Sender {
            tx: self.tx.clone(),
            closed_rx: self.closed_rx.clone(),
            remote_send_err_rx: self.remote_send_err_rx.clone(),
            _dropped_tx: self._dropped_tx.clone(),
            _codec: PhantomData,
        }
    }

    /// Sets the buffer size that will be used when sending this sender to a remote endpoint.
    pub fn set_buffer<const NEW_BUFFER: usize>(self) -> Sender<T, Codec, NEW_BUFFER> {
        Sender {
            tx: self.tx.clone(),
            closed_rx: self.closed_rx.clone(),
            remote_send_err_rx: self.remote_send_err_rx.clone(),
            _dropped_tx: self._dropped_tx.clone(),
            _codec: PhantomData,
        }
    }
}

/// Owned permit to send one value into the channel.
pub struct Permit<T>(tokio::sync::mpsc::OwnedPermit<Result<T, RecvError>>);

impl<T> Permit<T>
where
    T: Send,
{
    /// Sends a value using the reserved capacity.
    pub fn send(self, value: T) {
        self.0.send(Ok(value));
    }
}

impl<T, Codec, const BUFFER: usize> Drop for Sender<T, Codec, BUFFER> {
    fn drop(&mut self) {
        // empty
    }
}

impl<T, Codec, const BUFFER: usize> Serialize for Sender<T, Codec, BUFFER>
where
    T: RemoteSend,
    Codec: CodecT,
{
    /// Serializes this sender for sending over a chmux channel.
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let port = match self.tx.upgrade() {
            // Channel is open.
            Some(tx) => {
                // Prepare channel for takeover.
                let mut closed_rx = self.closed_rx.clone();
                let mut remote_send_err_rx = self.remote_send_err_rx.clone();

                Some(PortSerializer::connect(|connect| {
                    tokio::spawn(async move {
                        // Establish chmux channel.
                        let (mut raw_tx, raw_rx) = match connect.await {
                            Ok(tx_rx) => tx_rx,
                            Err(err) => {
                                let _ = tx.send(Err(RecvError::RemoteConnect(err))).await;
                                return;
                            }
                        };

                        // Decode raw received data using remote receiver.
                        let mut remote_rx = remote::Receiver::<Result<T, RecvError>, Codec>::new(raw_rx);

                        // Process events.
                        let mut close_sent = false;
                        loop {
                            tokio::select! {
                                biased;

                                // Channel closure requested locally.
                                res = closed_rx.changed() => {
                                    match res {
                                        Ok(()) if *closed_rx.borrow() && !close_sent => {
                                            let _ = raw_tx.send(vec![BACKCHANNEL_MSG_CLOSE].into()).await;
                                            close_sent = true;
                                        }
                                        Ok(()) => (),
                                        Err(_) => break,
                                    }
                                }

                                // Notify remote endpoint of error.
                                Ok(()) = remote_send_err_rx.changed() => {
                                    if remote_send_err_rx.borrow().as_ref().is_some() {
                                        let _ = raw_tx.send(vec![BACKCHANNEL_MSG_ERROR].into()).await;
                                    }
                                }

                                // Data received from remote endpoint.
                                res = remote_rx.recv() => {
                                    let value = match res {
                                        Ok(Some(value)) => value,
                                        Ok(None) => break,
                                        Err(err) => Err(RecvError::RemoteReceive(err)),
                                    };
                                    if tx.send(value).await.is_err() {
                                        break;
                                    }
                                }
                            }
                        }
                    })
                    .map(|_| ())
                    .boxed()
                })?)
            }
            None => {
                // Channel is closed.
                None
            }
        };

        // Encode chmux port number in transport type and serialize it.
        let transported = TransportedSender::<T, Codec> { port, data: PhantomData, codec: PhantomData };
        transported.serialize(serializer)
    }
}

impl<'de, T, Codec, const BUFFER: usize> Deserialize<'de> for Sender<T, Codec, BUFFER>
where
    T: RemoteSend,
    Codec: CodecT,
{
    /// Deserializes this sender after it has been received over a chmux channel.
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        assert!(BUFFER > 0, "BUFFER must not be zero");

        // Get chmux port number from deserialized transport type.
        let TransportedSender { port, .. } = TransportedSender::<T, Codec>::deserialize(deserializer)?;

        match port {
            // Received channel is open.
            Some(port) => {
                // Create internal communication channels.
                let (tx, mut rx) = tokio::sync::mpsc::channel(BUFFER);
                let (closed_tx, closed_rx) = tokio::sync::watch::channel(false);
                let (remote_send_err_tx, remote_send_err_rx) = tokio::sync::watch::channel(None);

                // Accept chmux port request.
                PortDeserializer::accept(port, |local_port, request| {
                    tokio::spawn(async move {
                        // Accept chmux connection request.
                        let (raw_tx, mut raw_rx) = match request.accept_from(local_port).await {
                            Ok(tx_rx) => tx_rx,
                            Err(err) => {
                                let _ = remote_send_err_tx.send(Some(RemoteSendError::Listen(err)));
                                return;
                            }
                        };

                        // Encode data using remote sender for sending.
                        let mut remote_tx = remote::Sender::<Result<T, RecvError>, Codec>::new(
                            raw_tx,
                        );

                        // Process events.
                        let mut backchannel_active = true;
                        loop {
                            tokio::select! {
                                biased;

                                // Backchannel message from remote endpoint.
                                backchannel_msg = raw_rx.recv(), if backchannel_active => {
                                    match backchannel_msg {
                                        Ok(Some(mut msg)) if msg.remaining() >= 1 => {
                                            match msg.get_u8() {
                                                BACKCHANNEL_MSG_CLOSE => {
                                                    let _ = closed_tx.send(true);
                                                }
                                                BACKCHANNEL_MSG_ERROR => {
                                                    let _ = remote_send_err_tx.send(Some(RemoteSendError::Forward));
                                                }
                                                _ => (),
                                            }
                                        },
                                        _ => backchannel_active = false,
                                    }
                                }

                                // Data to send to remote endpoint.
                                value_opt = rx.recv() => {
                                    match value_opt {
                                        Some(value) => {
                                            if let Err(err) = remote_tx.send(value).await {
                                                let _ = remote_send_err_tx.send(Some(RemoteSendError::Send(err.kind)));
                                            }
                                        }
                                        None => break,
                                    }
                                }
                            }
                        }
                    })
                    .map(|_| ())
                    .boxed()
                })?;

                Ok(Self::new(tx, closed_rx, remote_send_err_rx))
            }

            // Received closed channel.
            None => Ok(Self::new_closed()),
        }
    }
}
