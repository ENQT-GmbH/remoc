use bytes::Buf;
use futures::FutureExt;
use serde::{Deserialize, Serialize};
use std::{error::Error, fmt, marker::PhantomData};

use super::{
    super::{
        remote::{self, PortDeserializer, PortSerializer},
        RemoteSendError, BACKCHANNEL_MSG_ERROR,
    },
    Ref, ERROR_QUEUE,
};
use crate::{chmux, codec::CodecT, rsync::RemoteSend};

/// An error occured during receiving over a watch channel.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum RecvError {
    /// Receiving from a remote endpoint failed.
    RemoteReceive(remote::RecvError),
    /// Connecting a sent channel failed.
    RemoteConnect(chmux::ConnectError),
    /// Listening for a connection from a received channel failed.
    RemoteListen(chmux::ListenerError),
}

impl fmt::Display for RecvError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Self::RemoteReceive(err) => write!(f, "receive error: {}", err),
            Self::RemoteConnect(err) => write!(f, "connect error: {}", err),
            Self::RemoteListen(err) => write!(f, "listen error: {}", err),
        }
    }
}

impl Error for RecvError {}

/// An error occured during waiting for a change on a watch channel.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum ChangedError {
    /// The sender has been dropped or the connection has been lost.
    Closed,
}

impl ChangedError {
    /// True, if remote endpoint has closed channel.
    pub fn is_closed(&self) -> bool {
        matches!(self, Self::Closed)
    }
}

impl fmt::Display for ChangedError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Self::Closed => write!(f, "closed"),
        }
    }
}

impl Error for ChangedError {}

/// Receive values from the associated [Sender](super::Sender),
/// which may be located on a remote endpoint.
///
/// Instances are created by the [channel](super::channel) function.
#[derive(Clone)]
pub struct Receiver<T, Codec> {
    rx: tokio::sync::watch::Receiver<Result<T, RecvError>>,
    remote_send_err_tx: tokio::sync::mpsc::Sender<RemoteSendError>,
    _codec: PhantomData<Codec>,
}

impl<T, Codec> fmt::Debug for Receiver<T, Codec> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("Receiver").finish_non_exhaustive()
    }
}

/// Watch receiver in transport.
#[derive(Serialize, Deserialize)]
pub struct TransportedReceiver<T, Codec> {
    /// chmux port number.
    port: u32,
    /// Current data value.
    data: Result<T, RecvError>,
    /// Data codec.
    codec: PhantomData<Codec>,
}

impl<T, Codec> Receiver<T, Codec> {
    pub(crate) fn new(
        rx: tokio::sync::watch::Receiver<Result<T, RecvError>>,
        remote_send_err_tx: tokio::sync::mpsc::Sender<RemoteSendError>,
    ) -> Self {
        Self { rx, remote_send_err_tx, _codec: PhantomData }
    }

    /// Returns a reference to the most recently received value.
    pub fn borrow(&self) -> Result<Ref<'_, T>, RecvError> {
        let ref_res = self.rx.borrow();
        match &*ref_res {
            Ok(_) => Ok(Ref(ref_res)),
            Err(err) => Err(err.clone()),
        }
    }

    /// Returns a reference to the most recently received value and mark that value as seen.
    pub fn borrow_and_update(&mut self) -> Result<Ref<'_, T>, RecvError> {
        let ref_res = self.rx.borrow_and_update();
        match &*ref_res {
            Ok(_) => Ok(Ref(ref_res)),
            Err(err) => Err(err.clone()),
        }
    }

    /// Wait for a change notification, then mark the newest value as seen.
    pub async fn changed(&mut self) -> Result<(), ChangedError> {
        self.rx.changed().await.map_err(|_| ChangedError::Closed)
    }
}

impl<T, Codec> Drop for Receiver<T, Codec> {
    fn drop(&mut self) {
        // empty
    }
}

impl<T, Codec> Serialize for Receiver<T, Codec>
where
    T: RemoteSend + Sync + Clone,
    Codec: CodecT,
{
    /// Serializes this receiver for sending over a chmux channel.
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        // Prepare channel for takeover.
        let mut rx = self.rx.clone();
        let remote_send_err_tx = self.remote_send_err_tx.clone();

        let port = PortSerializer::connect(|connect| {
            tokio::spawn(async move {
                // Establish chmux channel.
                let (raw_tx, mut raw_rx) = match connect.await {
                    Ok(tx_rx) => tx_rx,
                    Err(err) => {
                        let _ = remote_send_err_tx.try_send(RemoteSendError::Connect(err));
                        return;
                    }
                };

                // Encode data using remote sender.
                let mut remote_tx = remote::Sender::<Result<T, RecvError>, Codec>::new(raw_tx);

                // Process events.
                let mut backchannel_active = true;
                loop {
                    tokio::select! {
                        biased;

                        // Backchannel message from remote endpoint.
                        backchannel_msg = raw_rx.recv(), if backchannel_active => {
                            match backchannel_msg {
                                Ok(Some(mut msg)) if msg.remaining() >= 1 => {
                                    if msg.get_u8() == BACKCHANNEL_MSG_ERROR {
                                        let _ = remote_send_err_tx.try_send(RemoteSendError::Forward);
                                    }
                                }
                                _ => backchannel_active = false,
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
            })
            .map(|_| ())
            .boxed()
        })?;

        // Encode chmux port number in transport type and serialize it.
        let data = self.rx.borrow().clone();
        let transported = TransportedReceiver::<T, Codec> { port, data, codec: PhantomData };
        transported.serialize(serializer)
    }
}

impl<'de, T, Codec> Deserialize<'de> for Receiver<T, Codec>
where
    T: RemoteSend + Sync,
    Codec: CodecT,
{
    /// Deserializes the receiver after it has been received over a chmux channel.
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        // Get chmux port number from deserialized transport type.
        let TransportedReceiver { port, data, .. } = TransportedReceiver::<T, Codec>::deserialize(deserializer)?;

        // Create channels.
        let (tx, rx) = tokio::sync::watch::channel(data);
        let (remote_send_err_tx, mut remote_send_err_rx) = tokio::sync::mpsc::channel(ERROR_QUEUE);

        PortDeserializer::accept(port, |local_port, request| {
            tokio::spawn(async move {
                // Accept chmux connection request.
                let (mut raw_tx, raw_rx) = match request.accept_from(local_port).await {
                    Ok(tx_rx) => tx_rx,
                    Err(err) => {
                        let _ = tx.send(Err(RecvError::RemoteListen(err)));
                        return;
                    }
                };

                // Decode received data using remote receiver.
                let mut remote_rx = remote::Receiver::<Result<T, RecvError>, Codec>::new(raw_rx);

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
            })
            .map(|_| ())
            .boxed()
        })?;

        Ok(Self::new(rx, remote_send_err_tx))
    }
}