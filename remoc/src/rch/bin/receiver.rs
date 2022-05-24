use futures::FutureExt;
use serde::{ser, Deserialize, Serialize};
use std::{
    fmt,
    sync::{Arc, Mutex},
};

use super::{
    super::{
        base::{PortDeserializer, PortSerializer},
        ConnectError,
    },
    Interlock, Location,
};
use crate::chmux;

/// A binary channel receiver.
pub struct Receiver {
    pub(super) receiver: Option<Result<chmux::Receiver, ConnectError>>,
    pub(super) sender_tx: Option<tokio::sync::mpsc::UnboundedSender<Result<chmux::Sender, ConnectError>>>,
    pub(super) receiver_rx: tokio::sync::mpsc::UnboundedReceiver<Result<chmux::Receiver, ConnectError>>,
    pub(super) interlock: Arc<Mutex<Interlock>>,
}

impl fmt::Debug for Receiver {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("Receiver").finish()
    }
}

/// A chmux channel receiver in transport.
#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct TransportedReceiver {
    /// chmux port number.
    pub port: u32,
}

impl Receiver {
    async fn connect(&mut self) {
        if self.receiver.is_none() {
            self.receiver = Some(self.receiver_rx.recv().await.unwrap_or(Err(ConnectError::Dropped)));
        }
    }

    /// Establishes the connection and returns a reference to the chmux receiver channel
    /// to the remote endpoint.
    pub async fn get(&mut self) -> Result<&mut chmux::Receiver, ConnectError> {
        self.connect().await;
        self.receiver.as_mut().unwrap().as_mut().map_err(|err| err.clone())
    }

    /// Establishes the connection and returns the chmux receiver channel
    /// to the remote endpoint.
    pub async fn into_inner(mut self) -> Result<chmux::Receiver, ConnectError> {
        self.connect().await;
        self.receiver.unwrap()
    }
}

impl Serialize for Receiver {
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
                        let _ = sender_tx.send(Ok(raw_tx));
                    }
                    Err(err) => {
                        let _ = sender_tx.send(Err(ConnectError::Connect(err)));
                    }
                }
            }
            .boxed()
        })?;

        TransportedReceiver { port }.serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for Receiver {
    /// Deserializes this receiver after it has been received over a chmux channel.
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let TransportedReceiver { port } = TransportedReceiver::deserialize(deserializer)?;

        let (receiver_tx, receiver_rx) = tokio::sync::mpsc::unbounded_channel();
        PortDeserializer::accept(port, |local_port, request| {
            async move {
                match request.accept_from(local_port).await {
                    Ok((_, raw_rx)) => {
                        let _ = receiver_tx.send(Ok(raw_rx));
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
