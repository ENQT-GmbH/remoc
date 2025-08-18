//! Binary channel with remotely sendable and forwardable sender.

use serde::{ser, Deserialize, Serialize};
use std::sync::Mutex;
use tracing::Instrument;

use crate::{exec, rch::bin};

/// A chmux sender that can be remotely sent and forwarded.
pub(crate) struct Sender {
    bin_tx: Mutex<Option<bin::Sender>>,
    bin_rx_tx: Mutex<Option<tokio::sync::oneshot::Sender<bin::Receiver>>>,
}

impl Sender {
    pub fn into_inner(self) -> Option<bin::Sender> {
        let mut bin_tx = self.bin_tx.lock().unwrap();
        bin_tx.take()
    }
}

/// A chmux sender that can be remotely sent and forwarded in transport form.
#[derive(Serialize, Deserialize)]
pub(crate) struct TransportedSender {
    bin_tx: bin::Sender,
}

impl Serialize for Sender {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let mut bin_tx = self.bin_tx.lock().unwrap();
        let mut bin_rx_tx = self.bin_rx_tx.lock().unwrap();

        match (bin_tx.take(), bin_rx_tx.take()) {
            // Initial send.
            (None, Some(bin_rx_tx)) => {
                let (bin_tx, bin_rx) = bin::channel();
                let _ = bin_rx_tx.send(bin_rx);
                TransportedSender { bin_tx }.serialize(serializer)
            }

            // Forwarded send.
            (Some(bin_tx), None) => {
                let (bin_fw_tx, bin_fw_rx) = bin::channel();
                exec::spawn(
                    async move {
                        let Ok(mut bin_tx) = bin_tx.into_inner().await else { return };
                        let Ok(mut bin_fw_rx) = bin_fw_rx.into_inner().await else { return };

                        // No error handling is performed, because complete transmission of
                        // data is verified by size.
                        let _ = bin_fw_rx.forward(&mut bin_tx).await;
                    }
                    .in_current_span(),
                );
                TransportedSender { bin_tx: bin_fw_tx }.serialize(serializer)
            }

            _ => Err(ser::Error::custom("invalid state")),
        }
    }
}

impl<'de> Deserialize<'de> for Sender {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let TransportedSender { bin_tx } = TransportedSender::deserialize(deserializer)?;

        Ok(Self { bin_tx: Mutex::new(Some(bin_tx)), bin_rx_tx: Mutex::new(None) })
    }
}

/// A receiver for the corresponding [Sender].
///
/// Cannot be remotely sent.
pub(crate) struct Receiver {
    bin_rx_rx: tokio::sync::oneshot::Receiver<bin::Receiver>,
}

impl Receiver {
    pub async fn into_inner(self) -> Option<bin::Receiver> {
        self.bin_rx_rx.await.ok()
    }
}

/// Create a binary channel with a sender that is remotely sendable and forwardable.
pub(crate) fn channel() -> (Sender, Receiver) {
    let (bin_rx_tx, bin_rx_rx) = tokio::sync::oneshot::channel();
    let sender = Sender { bin_tx: Mutex::new(None), bin_rx_tx: Mutex::new(Some(bin_rx_tx)) };
    let receiver = Receiver { bin_rx_rx };
    (sender, receiver)
}
