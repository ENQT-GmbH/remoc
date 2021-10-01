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
        remote::{self, PortDeserializer, PortSerializer},
        ConnectError,
    },
    Interlock, Location,
};
use crate::{
    chmux,
    codec::{CodecT, SerializationError},
};

/// An error that occured during remote sending.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SendError<T> {
    /// Error kind.
    pub kind: SendErrorKind,
    /// Item that could not be sent.
    pub item: T,
}

/// Error kind that occured during remote sending.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SendErrorKind {
    /// Serialization of the item failed.
    Serialize(SerializationError),
    /// Sending of the serialized item over the chmux channel failed.
    Send(chmux::SendError),
    /// Connecting to remote channel failed.
    Connect(ConnectError),
}

impl<T> SendError<T> {
    pub(crate) fn new(kind: SendErrorKind, item: T) -> Self {
        Self { kind, item }
    }
}

impl fmt::Display for SendErrorKind {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Self::Serialize(err) => write!(f, "serialization error: {}", err),
            Self::Send(err) => write!(f, "send error: {}", err),
            Self::Connect(err) => write!(f, "connect error: {}", err),
        }
    }
}

impl From<remote::SendErrorKind> for SendErrorKind {
    fn from(err: remote::SendErrorKind) -> Self {
        match err {
            remote::SendErrorKind::Serialize(err) => Self::Serialize(err),
            remote::SendErrorKind::Send(err) => Self::Send(err),
        }
    }
}

impl<T> fmt::Display for SendError<T> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", &self.kind)
    }
}

impl<T> From<remote::SendError<T>> for SendError<T> {
    fn from(err: remote::SendError<T>) -> Self {
        Self { kind: err.kind.into(), item: err.item }
    }
}

impl<T> Error for SendError<T> where T: fmt::Debug {}

/// The sender part of a local/remote channel.
pub struct Sender<T, Codec> {
    pub(super) sender: Option<Result<remote::Sender<T, Codec>, ConnectError>>,
    pub(super) sender_rx: tokio::sync::mpsc::UnboundedReceiver<Result<remote::Sender<T, Codec>, ConnectError>>,
    pub(super) receiver_tx:
        Option<tokio::sync::mpsc::UnboundedSender<Result<remote::Receiver<T, Codec>, ConnectError>>>,
    pub(super) interlock: Arc<Mutex<Interlock>>,
}

/// A local/remote channel sender in transport.
#[derive(Debug, Serialize, Deserialize)]
pub struct TransportedSender<T, Codec> {
    /// chmux port number.
    pub port: u32,
    /// Data type.
    data: PhantomData<T>,
    /// Data codec.
    codec: PhantomData<Codec>,
}

impl<T, Codec> Sender<T, Codec>
where
    T: Serialize + Send + 'static,
    Codec: CodecT,
{
    async fn connect(&mut self) {
        if self.sender.is_none() {
            self.sender = Some(self.sender_rx.recv().await.unwrap_or(Err(ConnectError::Dropped)));
        }
    }

    /// Establishes the connection and returns a reference to the remote sender.
    async fn get(&mut self) -> Result<&mut remote::Sender<T, Codec>, ConnectError> {
        self.connect().await;
        self.sender.as_mut().unwrap().as_mut().map_err(|err| err.clone())
    }

    /// Sends an item over the channel.
    ///
    /// The item may contain ports that will be serialized and connected as well.
    pub async fn send(&mut self, item: T) -> Result<(), SendError<T>> {
        match self.get().await {
            Ok(sender) => Ok(sender.send(item).await?),
            Err(err) => Err(SendError::new(SendErrorKind::Connect(err), item)),
        }
    }
}

impl<T, Codec> Serialize for Sender<T, Codec>
where
    T: DeserializeOwned + Send + 'static,
    Codec: CodecT,
{
    /// Serializes this sender for sending over a chmux channel.
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let receiver_tx =
            self.receiver_tx.clone().ok_or_else(|| ser::Error::custom("cannot forward received sender"))?;

        let interlock_confirm = {
            let mut interlock = self.interlock.lock().unwrap();
            if !interlock.receiver.is_local() {
                return Err(ser::Error::custom("cannot send sender because receiver has been sent"));
            }
            interlock.receiver.start_send()
        };

        let port = PortSerializer::connect(|connect, allocator| {
            async move {
                let _ = interlock_confirm.send(());

                match connect.await {
                    Ok((_, raw_rx)) => {
                        let rx = remote::Receiver::new(raw_rx, allocator);
                        let _ = receiver_tx.send(Ok(rx));
                    }
                    Err(err) => {
                        let _ = receiver_tx.send(Err(ConnectError::Connect(err)));
                    }
                }
            }
            .boxed()
        })?;

        TransportedSender::<T, Codec> { port, data: PhantomData, codec: PhantomData }.serialize(serializer)
    }
}

impl<'de, T, Codec> Deserialize<'de> for Sender<T, Codec>
where
    T: Serialize + Send + 'static,
    Codec: CodecT,
{
    /// Deserializes this sender after it has been received over a chmux channel.
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let TransportedSender::<T, Codec> { port, .. } = TransportedSender::deserialize(deserializer)?;

        let (sender_tx, sender_rx) = tokio::sync::mpsc::unbounded_channel();
        PortDeserializer::accept(port, |local_port, request, allocator| {
            async move {
                match request.accept_from(local_port).await {
                    Ok((raw_tx, _)) => {
                        let tx = remote::Sender::new(raw_tx, allocator);
                        let _ = sender_tx.send(Ok(tx));
                    }
                    Err(err) => {
                        let _ = sender_tx.send(Err(ConnectError::Listen(err)));
                    }
                }
            }
            .boxed()
        })?;

        Ok(Self {
            sender: None,
            sender_rx,
            receiver_tx: None,
            interlock: Arc::new(Mutex::new(Interlock { sender: Location::Local, receiver: Location::Remote })),
        })
    }
}
