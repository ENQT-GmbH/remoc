use std::sync::{Arc, Mutex};
use std::collections::VecDeque;
use futures::channel::{oneshot, mpsc};
use futures::sink::SinkExt;

use crate::receiver::ChannelReceiveError;
use crate::multiplexer::ChannelMsg;


/// Closed reason for channel receive buffer.
enum ChannelReceiverBufferCloseReason {
    /// Remote endpoint dropped sender.
    Closed,
    /// Multiplexer was dropped.
    Dropped
}

/// Internal state of channel receiver buffer.
struct ChannelReceiverBufferState<Content> {
    buffer: VecDeque<Content>,
    enqueuer_close_reason: Option<ChannelReceiverBufferCloseReason>,
    dequeuer_dropped: bool,
    item_enqueued: Option<oneshot::Sender<()>>,
    item_dequeued: Option<oneshot::Sender<()>>,
}

/// Channel receiver buffer item enqueuer.
pub struct ChannelReceiverBufferEnqueuer<Content> {
    state: Arc<Mutex<ChannelReceiverBufferState<Content>>>,
    resume_length: usize,
    pause_length: usize,
    block_length: usize,
}


impl<Content> ChannelReceiverBufferEnqueuer<Content> {
    /// Enqueues an item into the receive queue.
    /// Blocks when the block queue length has been reached.
    /// Returns true when the pause queue length has been reached from below.
    pub async fn enqueue(&self, item: Content) -> bool {
        let mut rx_opt = None;
        loop {
            if let Some(rx) = rx_opt {
                let _ = rx.await;
            }

            let mut state = self.state.lock().unwrap();
            if state.dequeuer_dropped {
                // Drop item when dequeuer has been dropped.
                return false;
            }
            if state.buffer.len() >= self.block_length {
                let (tx, rx) = oneshot::channel();
                state.item_dequeued = Some(tx);
                rx_opt = Some(rx);
                continue;
            }

            state.buffer.push_back(item);
            if let Some(tx) = state.item_enqueued.take() {
                let _ = tx.send(());
            }

            return state.buffer.len() == self.pause_length;
        }
    }

    /// Returns true, if buffer length is at or below resume length.
    pub fn resumeable(&self) -> bool {
        let state = self.state.lock().unwrap();
        state.buffer.len() <= self.resume_length
    }

    /// Indicates that the receive stream is finished.
    pub fn close(self) {
        let mut state = self.state.lock().unwrap();
        state.enqueuer_close_reason = Some(ChannelReceiverBufferCloseReason::Closed);
        if let Some(tx) = state.item_enqueued.take() {
            let _ = tx.send(());
        }            
    }
}

impl<Content> Drop for ChannelReceiverBufferEnqueuer<Content> {
    fn drop(&mut self) {
        let mut state = self.state.lock().unwrap();
        if state.enqueuer_close_reason.is_none() {
            state.enqueuer_close_reason = Some(ChannelReceiverBufferCloseReason::Dropped);
            if let Some(tx) = state.item_enqueued.take() {
                let _ = tx.send(());
            }            
        }
    }
}


/// Channel receiver buffer item dequeuer.
pub struct ChannelReceiverBufferDequeuer<Content> {
    state: Arc<Mutex<ChannelReceiverBufferState<Content>>>,
    resume_length: usize,
    resume_notify_tx: mpsc::Sender<ChannelMsg<Content>>,
    local_port: u32,
}

impl<Content> ChannelReceiverBufferDequeuer<Content> {
    /// Dequeues an item from the receive queue.
    /// Blocks until an item becomes available.
    /// Notifies the resume notify channel when the resume queue length has been reached from above.
    pub async fn dequeue(&mut self) -> Option<Result<Content, ChannelReceiveError>> {
        let mut rx_opt = None;
        loop {
            if let Some(rx) = rx_opt {
                let _ = rx.await;
            }

            let mut state = self.state.lock().unwrap();
            if state.buffer.is_empty() {
                match &state.enqueuer_close_reason {
                    Some (ChannelReceiverBufferCloseReason::Closed) => return None,
                    Some (ChannelReceiverBufferCloseReason::Dropped) => return Some(Err(ChannelReceiveError::MultiplexerError)),
                    None => {                
                        let (tx, rx) = oneshot::channel();
                        state.item_enqueued = Some(tx);
                        rx_opt = Some(rx);
                        continue;
                    }
                }
            }

            let item = state.buffer.pop_front().unwrap();
            if let Some(tx) = state.item_dequeued.take() {
                let _ = tx.send(());
            }

            if state.buffer.len() == self.resume_length {
                let _ = self.resume_notify_tx
                    .send(ChannelMsg::ReceiveBufferReachedResumeLength {local_port: self.local_port})
                    .await;
            }

            return Some(Ok(item));
        }
    }
}

impl<Content> Drop for ChannelReceiverBufferDequeuer<Content> {
    fn drop(&mut self) {
        let mut state = self.state.lock().unwrap();
        state.dequeuer_dropped = true;
        if let Some(tx) = state.item_dequeued.take() {
            let _ = tx.send(());
        }            
    }
}


/// Channel receive buffer configuration.
pub struct ChannelReceiverBufferCfg<Content> {
    /// Buffer length that will trigger sending resume notification.
    pub resume_length: usize,
    /// Buffer length that will trigger sending pause notification.
    pub pause_length: usize,
    /// When buffer length reaches this value, enqueue function will block.
    pub block_length: usize,
    /// Resume notification sender.
    pub resume_notify_tx: mpsc::Sender<ChannelMsg<Content>>,
    /// Local port for resume notification.
    pub local_port: u32,
}


impl<Content> ChannelReceiverBufferCfg<Content> where Content: 'static {
    /// Creates a new channel receiver buffer and returns the associated
    /// enqueuer and dequeuer.
    pub fn instantiate(self) -> (
        ChannelReceiverBufferEnqueuer<Content>,
        ChannelReceiverBufferDequeuer<Content>,
    ) {
        assert!(self.resume_length > 0);
        assert!(self.pause_length > self.resume_length);
        assert!(self.block_length > self.pause_length);
    
        let state = Arc::new(Mutex::new(ChannelReceiverBufferState {
            buffer: VecDeque::new(),
            enqueuer_close_reason: None,
            dequeuer_dropped: false,
            item_enqueued: None,
            item_dequeued: None,
        }));
    
        let enqueuer = ChannelReceiverBufferEnqueuer {
            state: state.clone(),
            resume_length: self.resume_length,
            pause_length: self.pause_length,
            block_length: self.block_length,
        };
        let dequeuer = ChannelReceiverBufferDequeuer {
            state: state.clone(),
            resume_length: self.resume_length,
            resume_notify_tx: self.resume_notify_tx,
            local_port: self.local_port,
        };
        (enqueuer, dequeuer)
    }
}
