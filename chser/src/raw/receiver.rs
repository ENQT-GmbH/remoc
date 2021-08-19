use std::sync::{Arc, Mutex};

use futures::FutureExt;
use serde::{ser, Deserialize, Serialize};

use super::{ConnectError, Interlock, Location};
use crate::remote::{PortDeserializer, PortSerializer};

/// A raw chmux channel receiver.
pub struct Receiver {
    pub(super) sender_tx: Option<tokio::sync::mpsc::UnboundedSender<Result<chmux::Sender, ConnectError>>>,
    pub(super) receiver_rx: tokio::sync::mpsc::UnboundedReceiver<Result<chmux::Receiver, ConnectError>>,
    pub(super) interlock: Arc<Mutex<Interlock>>,
}

/// A raw chmux channel receiver in transport.
#[derive(Debug, Serialize, Deserialize)]
pub struct TransportedReceiver {
    /// chmux port number.
    pub port: u32,
}

impl Receiver {
    /// Establishes the connection and returns the chmux receiver channel
    /// from the remote endpoint.
    pub async fn connect(mut self) -> Result<chmux::Receiver, ConnectError> {
        self.receiver_rx.recv().await.unwrap_or(Err(ConnectError::Dropped))
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
            if !interlock.sender.is_local() {
                return Err(ser::Error::custom("cannot send receiver because sender has been sent"));
            }
            interlock.sender.start_send()
        };

        let port = PortSerializer::connect(|connect, _| {
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
        PortDeserializer::accept(port, |local_port, request, _| {
            async move {
                match request.accept_from(local_port).await {
                    Ok((_, raw_rx)) => {
                        let _ = receiver_tx.send(Ok(raw_rx));
                    }
                    Err(err) => {
                        let _ = receiver_tx.send(Err(ConnectError::Accept(err)));
                    }
                }
            }
            .boxed()
        })?;

        Ok(Self {
            sender_tx: None,
            receiver_rx,
            interlock: Arc::new(Mutex::new(Interlock { sender: Location::Remote, receiver: Location::Local })),
        })
    }
}
