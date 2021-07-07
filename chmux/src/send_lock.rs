use futures::{
    channel::{mpsc, oneshot},
    future::FutureExt,
    lock::Mutex,
    select,
    stream::StreamExt,
};
use std::sync::Arc;

use crate::sender::SendError;

/// Sets the send lock state of a channel.
pub struct ChannelSendLockAuthority {
    state: Arc<Mutex<ChannelSendLockState>>,
    _authority_dropped_tx: mpsc::Sender<()>,
}

/// Gets the send lock state of a channel.
pub struct ChannelSendLockRequester {
    state: Arc<Mutex<ChannelSendLockState>>,
    authority_dropped: bool,
    authority_dropped_rx: mpsc::Receiver<()>,
}

/// Internal state of a channel send lock.
struct ChannelSendLockState {
    /// Is sending currently allowed or paused by the remote endpoint?
    send_allowed: bool,
    /// If closed by authority, if closure was graceful.
    closed: Option<bool>,
    /// Notification channel to event loop.
    notify_tx: Option<oneshot::Sender<()>>,
}

impl ChannelSendLockAuthority {
    /// Pause sending on that channel.
    pub async fn pause(&mut self) {
        let mut state = self.state.lock().await;
        state.send_allowed = false;
    }

    /// Resume sending on the channel.
    pub async fn resume(&mut self) {
        let mut state = self.state.lock().await;
        state.send_allowed = true;

        if let Some(tx) = state.notify_tx.take() {
            let _ = tx.send(());
        }
    }

    /// Closes the channel.
    pub async fn close(self, gracefully: bool) {
        let mut state = self.state.lock().await;
        state.send_allowed = false;
        state.closed = Some(gracefully);

        if let Some(tx) = state.notify_tx.take() {
            let _ = tx.send(());
        }
    }
}

impl Drop for ChannelSendLockAuthority {
    fn drop(&mut self) {
        // required for correct drop order
    }
}

impl ChannelSendLockRequester {
    /// Blocks until sending on the channel is allowed.
    pub async fn request(&mut self) -> Result<(), SendError> {
        let mut rx_opt: Option<oneshot::Receiver<()>> = None;
        loop {
            if let Some(rx) = rx_opt {
                select! {
                    _ = rx.fuse() => {},
                    _ = self.authority_dropped_rx.next() => {
                        self.authority_dropped = true;
                    }
                }
            }

            if self.authority_dropped {
                return Err(SendError::MultiplexerError);
            }

            let mut state = self.state.lock().await;
            if state.send_allowed {
                return Ok(());
            }
            if let Some(gracefully) = &state.closed {
                return Err(SendError::Closed { gracefully: *gracefully });
            }

            let (tx, rx) = oneshot::channel();
            state.notify_tx = Some(tx);
            rx_opt = Some(rx);
        }
    }
}

/// Creates a channel send lock.
pub fn channel_send_lock() -> (ChannelSendLockAuthority, ChannelSendLockRequester) {
    let (authority_dropped_tx, authority_dropped_rx) = mpsc::channel(1);
    let state = Arc::new(Mutex::new(ChannelSendLockState { send_allowed: true, closed: None, notify_tx: None }));
    let authority =
        ChannelSendLockAuthority { state: state.clone(), _authority_dropped_tx: authority_dropped_tx };
    let requester = ChannelSendLockRequester { state, authority_dropped: false, authority_dropped_rx };
    (authority, requester)
}
