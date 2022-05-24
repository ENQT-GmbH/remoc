use futures::FutureExt;
use serde::{de::DeserializeOwned, ser, Deserialize, Serialize};
use std::{
    error::Error,
    fmt,
    marker::PhantomData,
    sync::{Arc, Mutex},
};

use super::{
    super::{
        base::{self, PortDeserializer, PortSerializer},
        ConnectError,
    },
    Interlock, Location,
};
use crate::{
    chmux,
    codec::{self, DeserializationError},
};

/// An error that occurred during receiving from a remote endpoint.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum RecvError {
    /// Receiving data over the chmux channel failed.
    Receive(chmux::RecvError),
    /// Deserialization of received data failed.
    Deserialize(DeserializationError),
    /// chmux ports required for deserialization of received channels were not received.
    MissingPorts(Vec<u32>),
    /// Connecting to remote channel failed.
    Connect(ConnectError),
}

impl From<base::RecvError> for RecvError {
    fn from(err: base::RecvError) -> Self {
        match err {
            base::RecvError::Receive(err) => Self::Receive(err),
            base::RecvError::Deserialize(err) => Self::Deserialize(err),
            base::RecvError::MissingPorts(ports) => Self::MissingPorts(ports),
        }
    }
}

impl From<ConnectError> for RecvError {
    fn from(err: ConnectError) -> Self {
        Self::Connect(err)
    }
}

impl fmt::Display for RecvError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Self::Receive(err) => write!(f, "receive error: {}", err),
            Self::Deserialize(err) => write!(f, "deserialization error: {}", err),
            Self::MissingPorts(ports) => write!(
                f,
                "missing chmux ports: {}",
                ports.iter().map(|p| p.to_string()).collect::<Vec<_>>().join(", ")
            ),
            Self::Connect(err) => write!(f, "connect error: {}", err),
        }
    }
}

impl Error for RecvError {}

impl RecvError {
    /// Returns whether the error is final, i.e. no further receive operation can succeed.
    pub fn is_final(&self) -> bool {
        match self {
            Self::Receive(err) => err.is_final(),
            Self::Connect(_) => true,
            Self::Deserialize(_) | Self::MissingPorts(_) => false,
        }
    }
}

/// The receiver part of a local/remote channel.
pub struct Receiver<T, Codec = codec::Default> {
    pub(super) receiver: Option<Result<base::Receiver<T, Codec>, ConnectError>>,
    pub(super) sender_tx:
        Option<tokio::sync::mpsc::UnboundedSender<Result<base::Sender<T, Codec>, ConnectError>>>,
    pub(super) receiver_rx: tokio::sync::mpsc::UnboundedReceiver<Result<base::Receiver<T, Codec>, ConnectError>>,
    pub(super) interlock: Arc<Mutex<Interlock>>,
}

impl<T, Codec> fmt::Debug for Receiver<T, Codec> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("Receiver").finish()
    }
}

/// A raw chmux channel receiver in transport.
#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct TransportedReceiver<T, Codec> {
    /// chmux port number.
    pub port: u32,
    /// Data type.
    data: PhantomData<T>,
    /// Data codec.
    codec: PhantomData<Codec>,
}

impl<T, Codec> Receiver<T, Codec>
where
    T: DeserializeOwned + Send + 'static,
    Codec: codec::Codec,
{
    async fn connect(&mut self) {
        if self.receiver.is_none() {
            self.receiver = Some(self.receiver_rx.recv().await.unwrap_or(Err(ConnectError::Dropped)));
        }
    }

    /// Establishes the connection and returns a reference to the remote receiver.
    async fn get(&mut self) -> Result<&mut base::Receiver<T, Codec>, ConnectError> {
        self.connect().await;
        self.receiver.as_mut().unwrap().as_mut().map_err(|err| err.clone())
    }

    /// Receive an item from the remote endpoint.
    pub async fn recv(&mut self) -> Result<Option<T>, RecvError> {
        let receiver = self.get().await?;
        let item = receiver.recv().await?;
        Ok(item)
    }

    /// Close the channel.
    ///
    /// This stops the remote endpoint from sending more items, but allows already sent items
    /// to be received.    
    pub async fn close(&mut self) {
        if let Ok(receiver) = self.get().await {
            receiver.close().await;
        }
    }
}

impl<T, Codec> Serialize for Receiver<T, Codec>
where
    T: Serialize + Send + 'static,
    Codec: codec::Codec,
{
    /// Serializes this receiver for sending over a chmux channel.
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let sender_tx =
            self.sender_tx.clone().ok_or_else(|| ser::Error::custom("cannot forward received receiver"))?;

        let interlock_confirm = {
            let mut interlock = self.interlock.lock().unwrap();
            if !interlock.sender.check_local() {
                return Err(ser::Error::custom("cannot send receiver because sender has been sent"));
            }
            interlock.sender.start_send()
        };

        let port = PortSerializer::connect(|connect| {
            async move {
                let _ = interlock_confirm.send(());

                match connect.await {
                    Ok((raw_tx, _)) => {
                        let tx = base::Sender::new(raw_tx);
                        let _ = sender_tx.send(Ok(tx));
                    }
                    Err(err) => {
                        let _ = sender_tx.send(Err(ConnectError::Connect(err)));
                    }
                }
            }
            .boxed()
        })?;

        TransportedReceiver::<T, Codec> { port, data: PhantomData, codec: PhantomData }.serialize(serializer)
    }
}

impl<'de, T, Codec> Deserialize<'de> for Receiver<T, Codec>
where
    T: DeserializeOwned + Send + 'static,
    Codec: codec::Codec,
{
    /// Deserializes this receiver after it has been received over a chmux channel.
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let TransportedReceiver::<T, Codec> { port, .. } = TransportedReceiver::deserialize(deserializer)?;

        let (receiver_tx, receiver_rx) = tokio::sync::mpsc::unbounded_channel();
        PortDeserializer::accept(port, |local_port, request| {
            async move {
                match request.accept_from(local_port).await {
                    Ok((_, raw_rx)) => {
                        let rx = base::Receiver::new(raw_rx);
                        let _ = receiver_tx.send(Ok(rx));
                    }
                    Err(err) => {
                        let _ = receiver_tx.send(Err(ConnectError::Listen(err)));
                    }
                }
            }
            .boxed()
        })?;

        Ok(Self {
            receiver: None,
            sender_tx: None,
            receiver_rx,
            interlock: Arc::new(Mutex::new(Interlock { sender: Location::Remote, receiver: Location::Local })),
        })
    }
}
