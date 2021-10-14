use futures::future;

use super::{super::buffer, channel, Permit, Receiver, Sender};
use crate::{codec, RemoteSend};

struct DistributedReceiver<T, Codec, Buffer> {
    tx: Sender<T, Codec, Buffer>,
    remove_rx: Option<tokio::sync::mpsc::UnboundedReceiver<()>>,
}

impl<T, Codec, Buffer> DistributedReceiver<T, Codec, Buffer>
where
    T: RemoteSend + Clone,
    Codec: codec::Codec,
    Buffer: buffer::Size,
{
    async fn reserve(&mut self) -> Option<Permit<T>> {
        let tx = self.tx.clone();

        loop {
            let remove = async {
                match &mut self.remove_rx {
                    Some(remove_rx) => remove_rx.recv().await,
                    None => future::pending().await,
                }
            };

            tokio::select! {
                res = tx.reserve() => return res.ok(),
                res = remove => {
                    match res {
                        Some(()) => return None,
                        None => self.remove_rx = None,
                    }
                }
            }
        }
    }
}

/// A handle to a receiver that receives its values from a distributor.
pub struct DistributedReceiverHandle(tokio::sync::mpsc::UnboundedSender<()>);

impl DistributedReceiverHandle {
    /// Removes the associated receiver from the distributor.
    pub fn remove(self) {
        let _ = self.0.send(());
    }

    /// Waits for the associated receiver to be closed or fail due to an error.
    pub async fn closed(&mut self) {
        self.0.closed().await
    }
}

/// Distributes items of an mpsc channel over multiple receivers.
///
/// Distribution is stopped and all subscribers are closed when the distributor
/// is dropped.
pub struct Distributor<T, Codec = codec::Default, Buffer = buffer::Default> {
    #[allow(clippy::type_complexity)]
    sub_tx: tokio::sync::mpsc::Sender<
        tokio::sync::oneshot::Sender<(Receiver<T, Codec, Buffer>, DistributedReceiverHandle)>,
    >,
}

impl<T, Codec, Buffer> Distributor<T, Codec, Buffer>
where
    T: RemoteSend + Clone,
    Codec: codec::Codec,
    Buffer: buffer::Size,
{
    pub(crate) fn new(rx: Receiver<T, Codec, Buffer>, wait_on_empty: bool) -> Self {
        let (sub_tx, sub_rx) = tokio::sync::mpsc::channel(1);
        tokio::spawn(Self::distribute(rx, sub_rx, wait_on_empty));
        Self { sub_tx }
    }

    #[allow(clippy::type_complexity)]
    async fn distribute(
        mut rx: Receiver<T, Codec, Buffer>,
        mut sub_rx: tokio::sync::mpsc::Receiver<
            tokio::sync::oneshot::Sender<(Receiver<T, Codec, Buffer>, DistributedReceiverHandle)>,
        >,
        wait_on_empty: bool,
    ) {
        let mut txs: Vec<DistributedReceiver<T, Codec, Buffer>> = Vec::new();
        let mut first = true;

        loop {
            if txs.is_empty() && !(wait_on_empty || first) {
                return;
            }
            first = false;

            let send_task = async {
                if txs.is_empty() {
                    future::pending().await
                } else {
                    let permits = txs.iter_mut().map(|dr| Box::pin(dr.reserve()));
                    let (permit_opt, pos, _) = future::select_all(permits).await;

                    match permit_opt {
                        None => {
                            txs.swap_remove(pos);
                        }
                        Some(permit) => {
                            let value = match rx.recv().await {
                                Ok(Some(value)) => value,
                                _ => return false,
                            };
                            permit.send(value);
                        }
                    }

                    true
                }
            };

            tokio::select! {
                cont = send_task => {
                    if !cont {
                        return;
                    }
                }

                sub_opt = sub_rx.recv() => {
                    match sub_opt {
                        Some(sub_tx) => {
                            let (tx, rx) = channel(1);
                            let tx = tx.set_buffer();
                            let rx = rx.set_buffer();
                            let (remove_tx, remove_rx) = tokio::sync::mpsc::unbounded_channel();
                            let dr = DistributedReceiver {
                                tx, remove_rx: Some(remove_rx)
                            };
                            let drh = DistributedReceiverHandle(remove_tx);
                            txs.push(dr);
                            let _ = sub_tx.send((rx, drh));
                        }
                        None => return,
                    }
                }
            }
        }
    }

    /// Creates a new subscribed receiver and returns it along with its handle.
    pub async fn subscribe(&self) -> Option<(Receiver<T, Codec, Buffer>, DistributedReceiverHandle)> {
        let (sub_tx, sub_rx) = tokio::sync::oneshot::channel();
        let _ = self.sub_tx.send(sub_tx).await;
        sub_rx.await.ok()
    }

    /// Waits until the distributor is closed.
    ///
    /// The distributor closes when all subscribers are closed and `wait_on_empty` is false,
    /// or when the upstream sender is dropped or fails.
    pub async fn closed(&self) {
        self.sub_tx.closed().await
    }
}

impl<T, Codec, Buffer> Drop for Distributor<T, Codec, Buffer> {
    fn drop(&mut self) {
        // empty
    }
}
