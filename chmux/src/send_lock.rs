use std::sync::{Arc};
use futures::channel::{oneshot};
use futures::lock::Mutex;
use async_thread::on_thread;

use crate::sender::SendError;

/// Sets the send lock state of a channel.
pub struct ChannelSendLockAuthority {
    state: Arc<Mutex<ChannelSendLockState>>,
}

/// Gets the send lock state of a channel.
pub struct ChannelSendLockRequester {
    state: Arc<Mutex<ChannelSendLockState>>,
}

/// Reason for closing a channel send lock.
enum ChannelSendLockCloseReason {
    /// Remote endpoint closed or dropped receiver.
    Closed {gracefully: bool},
    /// Multiplexer was terminated.
    Dropped
}

/// Internal state of a channel send lock.
struct ChannelSendLockState {
    send_allowed: bool,
    close_reason: Option<ChannelSendLockCloseReason>,
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
        state.close_reason = Some(ChannelSendLockCloseReason::Closed {gracefully});

        if let Some(tx) = state.notify_tx.take() {
            let _ = tx.send(());
        }        
    }
}

impl Drop for ChannelSendLockAuthority {
    fn drop(&mut self) {
        on_thread(async{
            let mut state = self.state.lock().await;
            if state.close_reason.is_none() {
                state.send_allowed = false;            
                state.close_reason = Some(ChannelSendLockCloseReason::Dropped);

                if let Some(tx) = state.notify_tx.take() {
                    let _ = tx.send(());
                }                    
            }
        });
    }
}

impl ChannelSendLockRequester {
    /// Blocks until sending on the channel is allowed.
    pub async fn request(&self) -> Result<(), SendError> {
        let mut rx_opt = None;
        loop {
            if let Some(rx) = rx_opt {
                let _ = rx.await;
            }

            let mut state = self.state.lock().await;
            if state.send_allowed {
                return Ok(());
            } 
            match &state.close_reason {
                Some (ChannelSendLockCloseReason::Closed {gracefully}) => return Err(SendError::Closed {gracefully: gracefully.clone()}),
                Some (ChannelSendLockCloseReason::Dropped) => return Err(SendError::MultiplexerError),
                None => ()                
            }

            let (tx, rx) = oneshot::channel();
            state.notify_tx = Some(tx);
            rx_opt = Some(rx);
        }
    }
}

/// Creates a channel send lock.
pub fn channel_send_lock() -> (ChannelSendLockAuthority, ChannelSendLockRequester) {
    let state = Arc::new(Mutex::new(ChannelSendLockState {
        send_allowed: true,
        close_reason: None,
        notify_tx: None,
    }));
    let authority = ChannelSendLockAuthority {
        state: state.clone(),
    };
    let requester = ChannelSendLockRequester {
        state: state.clone(),
    };
    (authority, requester)
}

