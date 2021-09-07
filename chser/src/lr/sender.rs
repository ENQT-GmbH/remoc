use std::sync::{Arc, Mutex};

use futures::FutureExt;
use serde::{ser, Deserialize, Serialize};

use super::{ConnectError, Interlock, Location};
use crate::{
    codec::CodecT,
    remote::{self, PortDeserializer, PortSerializer},
};

/// A raw chmux channel sender.
pub struct Sender<T, Codec> {
    pub(super) sender: Option<Result<remote::Sender<T, Codec>, ConnectError>>,
    pub(super) sender_rx: tokio::sync::mpsc::UnboundedReceiver<Result<remote::Sender<T, Codec>, ConnectError>>,
    pub(super) receiver_tx: Option<tokio::sync::mpsc::UnboundedSender<Result<chmux::Receiver, ConnectError>>>,
    pub(super) interlock: Arc<Mutex<Interlock>>,
}

/// A raw chmux channel sender in transport.
#[derive(Debug, Serialize, Deserialize)]
pub struct TransportedSender {
    /// chmux port number.
    pub port: u32,
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

    /// Establishes the connection and returns a reference to the remote sender channel
    /// to the remote endpoint.
    async fn get(&mut self) -> Result<&mut remote::Sender<T, Codec>, ConnectError> {
        self.connect().await;
        self.sender.as_mut().unwrap().as_mut().map_err(|err| err.clone())
    }

    /// Sends an item over the channel.
    ///
    /// The item may contain ports that will be serialized and connected as well.
    pub async fn send(&mut self, item: T) -> Result<(), remote::SendError<T>> {
        self.get().await?.send(item).await
    }
}

impl Serialize for Sender {
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

        let port = PortSerializer::connect(|connect, _| {
            async move {
                let _ = interlock_confirm.send(());

                match connect.await {
                    Ok((_, raw_rx)) => {
                        let _ = receiver_tx.send(Ok(raw_rx));
                    }
                    Err(err) => {
                        let _ = receiver_tx.send(Err(ConnectError::Connect(err)));
                    }
                }
            }
            .boxed()
        })?;

        TransportedSender { port }.serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for Sender {
    /// Deserializes this sender after it has been received over a chmux channel.
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let TransportedSender { port } = TransportedSender::deserialize(deserializer)?;

        let (sender_tx, sender_rx) = tokio::sync::mpsc::unbounded_channel();
        PortDeserializer::accept(port, |local_port, request, _| {
            async move {
                match request.accept_from(local_port).await {
                    Ok((raw_tx, _)) => {
                        let _ = sender_tx.send(Ok(raw_tx));
                    }
                    Err(err) => {
                        let _ = sender_tx.send(Err(ConnectError::Accept(err)));
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
