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
        ConnectError, SendErrorExt,
    },
    Interlock, Location,
};
use crate::{
    chmux,
    codec::{self, SerializationError},
};

pub use super::super::base::Closed;

/// An error that occurred during remote sending.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SendError<T> {
    /// Error kind.
    pub kind: SendErrorKind,
    /// Item that could not be sent.
    pub item: T,
}

/// Error kind that occurred during remote sending.
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

    /// True, if the remote endpoint closed the channel.
    pub fn is_closed(&self) -> bool {
        matches!(&self.kind, SendErrorKind::Send(err) if err.is_closed())
    }

    /// True, if the remote endpoint closed the channel, was dropped or the connection failed.
    pub fn is_disconnected(&self) -> bool {
        !matches!(&self.kind, SendErrorKind::Serialize(_))
    }

    /// Returns whether the error is final, i.e. no further send operation can succeed.
    pub fn is_final(&self) -> bool {
        self.is_disconnected()
    }

    /// Returns the error without the contained item.
    pub fn without_item(self) -> SendError<()> {
        SendError { kind: self.kind, item: () }
    }
}

impl<T> SendErrorExt for SendError<T> {
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

impl fmt::Display for SendErrorKind {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Self::Serialize(err) => write!(f, "serialization error: {}", err),
            Self::Send(err) => write!(f, "send error: {}", err),
            Self::Connect(err) => write!(f, "connect error: {}", err),
        }
    }
}

impl From<base::SendErrorKind> for SendErrorKind {
    fn from(err: base::SendErrorKind) -> Self {
        match err {
            base::SendErrorKind::Serialize(err) => Self::Serialize(err),
            base::SendErrorKind::Send(err) => Self::Send(err),
        }
    }
}

impl<T> fmt::Display for SendError<T> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", &self.kind)
    }
}

impl<T> From<base::SendError<T>> for SendError<T> {
    fn from(err: base::SendError<T>) -> Self {
        Self { kind: err.kind.into(), item: err.item }
    }
}

impl<T> Error for SendError<T> where T: fmt::Debug {}

/// The sender part of a local/remote channel.
pub struct Sender<T, Codec = codec::Default> {
    pub(super) sender: Option<Result<base::Sender<T, Codec>, ConnectError>>,
    pub(super) sender_rx: tokio::sync::mpsc::UnboundedReceiver<Result<base::Sender<T, Codec>, ConnectError>>,
    pub(super) receiver_tx:
        Option<tokio::sync::mpsc::UnboundedSender<Result<base::Receiver<T, Codec>, ConnectError>>>,
    pub(super) interlock: Arc<Mutex<Interlock>>,
}

impl<T, Codec> fmt::Debug for Sender<T, Codec> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("Sender").finish()
    }
}

/// A local/remote channel sender in transport.
#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct TransportedSender<T, Codec> {
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
    Codec: codec::Codec,
{
    /// Establishes the connection and returns a reference to the remote sender.
    async fn get(&mut self) -> Result<&mut base::Sender<T, Codec>, ConnectError> {
        if self.sender.is_none() {
            self.sender = Some(self.sender_rx.recv().await.unwrap_or(Err(ConnectError::Dropped)));
        }

        self.sender.as_mut().unwrap().as_mut().map_err(|err| err.clone())
    }

    /// Sends an item over the channel.
    pub async fn send(&mut self, item: T) -> Result<(), SendError<T>> {
        match self.get().await {
            Ok(sender) => Ok(sender.send(item).await?),
            Err(err) => Err(SendError::new(SendErrorKind::Connect(err), item)),
        }
    }

    /// True, once the remote endpoint has closed its receiver.
    pub async fn is_closed(&mut self) -> Result<bool, ConnectError> {
        Ok(self.get().await?.is_closed())
    }

    /// Returns a future that will resolve when the remote endpoint closes its receiver.
    pub async fn closed(&mut self) -> Result<Closed, ConnectError> {
        Ok(self.get().await?.closed())
    }
}

impl<T, Codec> Serialize for Sender<T, Codec>
where
    T: DeserializeOwned + Send + 'static,
    Codec: codec::Codec,
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
            if !interlock.receiver.check_local() {
                return Err(ser::Error::custom("cannot send sender because receiver has been sent"));
            }
            interlock.receiver.start_send()
        };

        let port = PortSerializer::connect(|connect| {
            async move {
                let _ = interlock_confirm.send(());

                match connect.await {
                    Ok((_, raw_rx)) => {
                        let rx = base::Receiver::new(raw_rx);
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
    Codec: codec::Codec,
{
    /// Deserializes this sender after it has been received over a chmux channel.
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let TransportedSender::<T, Codec> { port, .. } = TransportedSender::deserialize(deserializer)?;

        let (sender_tx, sender_rx) = tokio::sync::mpsc::unbounded_channel();
        PortDeserializer::accept(port, |local_port, request| {
            async move {
                match request.accept_from(local_port).await {
                    Ok((raw_tx, _)) => {
                        let tx = base::Sender::new(raw_tx);
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
