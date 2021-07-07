use futures::{
    channel::{mpsc, oneshot},
    future::FutureExt,
    lock::Mutex,
    select,
    sink::SinkExt,
    stream::StreamExt,
};
use std::{collections::VecDeque, sync::Arc};

use crate::{multiplexer::ChannelMsg, receiver::ReceiveError};

/// Internal state of channel receiver buffer.
struct ChannelReceiverBufferState<Content> {
    buffer: VecDeque<Content>,
    enqueuer_closed: bool,
    item_enqueued: Option<oneshot::Sender<()>>,
    item_dequeued: Option<oneshot::Sender<()>>,
}

/// Channel receiver buffer item enqueuer.
pub struct ChannelReceiverBufferEnqueuer<Content>
where
    Content: Send,
{
    state: Arc<Mutex<ChannelReceiverBufferState<Content>>>,
    resume_length: usize,
    pause_length: usize,
    block_length: usize,
    dequeuer_dropped_rx: mpsc::Receiver<()>,
    dequeuer_dropped: bool,
    _enqueuer_dropped_tx: mpsc::Sender<()>,
}

impl<Content> ChannelReceiverBufferEnqueuer<Content>
where
    Content: Send,
{
    /// Enqueues an item into the receive queue.
    /// Blocks when the block queue length has been reached.
    /// Returns true when the pause queue length has been reached from below.
    pub async fn enqueue(&mut self, item: Content) -> bool {
        let mut rx_opt: Option<oneshot::Receiver<()>> = None;
        loop {
            if let Some(rx) = rx_opt {
                select! {
                    _ = rx.fuse() => {},
                    _ = self.dequeuer_dropped_rx.next() => {
                        self.dequeuer_dropped = true;
                    }
                }
            }

            // Drop item when dequeuer has been dropped.
            if self.dequeuer_dropped {
                return false;
            }

            let mut state = self.state.lock().await;
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
    pub async fn resumeable(&self) -> bool {
        let state = self.state.lock().await;
        state.buffer.len() <= self.resume_length
    }

    /// Indicates that the receive stream is finished.
    pub async fn close(self) {
        let mut state = self.state.lock().await;
        state.enqueuer_closed = true;
        if let Some(tx) = state.item_enqueued.take() {
            let _ = tx.send(());
        }
    }
}

impl<Content> Drop for ChannelReceiverBufferEnqueuer<Content>
where
    Content: Send,
{
    fn drop(&mut self) {
        // required for correct drop order
    }
}

/// Channel receiver buffer item dequeuer.
pub struct ChannelReceiverBufferDequeuer<Content>
where
    Content: Send,
{
    state: Arc<Mutex<ChannelReceiverBufferState<Content>>>,
    resume_length: usize,
    resume_notify_tx: mpsc::Sender<ChannelMsg<Content>>,
    local_port: u32,
    enqueuer_dropped_rx: mpsc::Receiver<()>,
    enqueuer_dropped: bool,
    _dequeuer_dropped_tx: mpsc::Sender<()>,
}

impl<Content> ChannelReceiverBufferDequeuer<Content>
where
    Content: Send,
{
    /// Dequeues an item from the receive queue.
    /// Blocks until an item becomes available.
    /// Notifies the resume notify channel when the resume queue length has been reached from above.
    pub async fn dequeue(&mut self) -> Option<Result<Content, ReceiveError>> {
        let mut rx_opt: Option<oneshot::Receiver<()>> = None;
        loop {
            if let Some(rx) = rx_opt {
                select! {
                    _ = rx.fuse() => {},
                    _ = self.enqueuer_dropped_rx.next() => {
                        self.enqueuer_dropped = true;
                    }
                }
            }

            let mut state = self.state.lock().await;
            if state.buffer.is_empty() {
                if state.enqueuer_closed {
                    return None;
                } else if self.enqueuer_dropped {
                    return Some(Err(ReceiveError::MultiplexerError));
                } else {
                    let (tx, rx) = oneshot::channel();
                    state.item_enqueued = Some(tx);
                    rx_opt = Some(rx);
                    continue;
                }
            }

            let item = state.buffer.pop_front().unwrap();
            if let Some(tx) = state.item_dequeued.take() {
                let _ = tx.send(());
            }

            if state.buffer.len() == self.resume_length {
                let _ = self
                    .resume_notify_tx
                    .send(ChannelMsg::ReceiveBufferReachedResumeLength { local_port: self.local_port })
                    .await;
            }

            return Some(Ok(item));
        }
    }
}

impl<Content> Drop for ChannelReceiverBufferDequeuer<Content>
where
    Content: Send,
{
    fn drop(&mut self) {
        // required for correct drop order
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

impl<Content> ChannelReceiverBufferCfg<Content>
where
    Content: Send + 'static,
{
    /// Creates a new channel receiver buffer and returns the associated
    /// enqueuer and dequeuer.
    pub fn instantiate(self) -> (ChannelReceiverBufferEnqueuer<Content>, ChannelReceiverBufferDequeuer<Content>) {
        assert!(self.resume_length > 0);
        assert!(self.pause_length > self.resume_length);
        assert!(self.block_length > self.pause_length);

        let state = Arc::new(Mutex::new(ChannelReceiverBufferState {
            buffer: VecDeque::new(),
            enqueuer_closed: false,
            item_enqueued: None,
            item_dequeued: None,
        }));

        let (enqueuer_dropped_tx, enqueuer_dropped_rx) = mpsc::channel(1);
        let (dequeuer_dropped_tx, dequeuer_dropped_rx) = mpsc::channel(1);
        let enqueuer = ChannelReceiverBufferEnqueuer {
            state: state.clone(),
            resume_length: self.resume_length,
            pause_length: self.pause_length,
            block_length: self.block_length,
            dequeuer_dropped: false,
            dequeuer_dropped_rx,
            _enqueuer_dropped_tx: enqueuer_dropped_tx,
        };
        let dequeuer = ChannelReceiverBufferDequeuer {
            state,
            resume_length: self.resume_length,
            resume_notify_tx: self.resume_notify_tx,
            local_port: self.local_port,
            enqueuer_dropped: false,
            enqueuer_dropped_rx,
            _dequeuer_dropped_tx: dequeuer_dropped_tx,
        };
        (enqueuer, dequeuer)
    }
}
