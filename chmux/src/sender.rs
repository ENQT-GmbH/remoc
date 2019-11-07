use futures::channel::{mpsc};
use futures::executor::block_on;
use futures::sink::{Sink, SinkExt};
use futures::task::{Context, Poll};
use pin_project::{pin_project, pinned_drop};
use std::fmt;
use std::pin::Pin;
use std::sync::{Arc};

use crate::send_lock::{ChannelSendLockRequester};
use crate::multiplexer::ChannelMsg;

#[derive(Debug, Clone)]
pub enum ChannelSendError {
    /// Other side closed receiving end of channel.
    Closed { 
        /// True if other side still processes items send up until now.
        gracefully: bool,
    },
    /// The multiplexer encountered an error or was terminated.
    MultiplexerError
}

impl From<mpsc::SendError> for ChannelSendError
{
    fn from(err: mpsc::SendError) -> Self {
        if err.is_disconnected() {
            return Self::MultiplexerError;
        }
        panic!("Error sending data to multiplexer for unknown reasons.");
    }
}

#[pin_project]
pub struct ChannelSender<Content> {
    local_port: u32,
    remote_port: u32,
    #[pin]
    sink: Pin<Box<
        dyn Sink<
            Content,
            Error = ChannelSendError,
        >,
    >>,
    #[pin]
    drop_tx: mpsc::Sender<ChannelMsg<Content>>,
}

impl<Content>
    ChannelSender<Content>
{
    pub(crate) fn new(
        local_port: u32,
        remote_port: u32,
        tx: mpsc::Sender<ChannelMsg<Content>>,
        tx_lock: ChannelSendLockRequester,
    ) -> ChannelSender<Content>
    where
        Content: 'static,
    {
        let drop_tx = tx.clone();
        let tx_lock = Arc::new(tx_lock);
        let adapted_tx = tx.with(move |item: Content| {
            let tx_lock = tx_lock.clone();
            async move {
                tx_lock.request().await?;
                let msg = ChannelMsg::SendMsg {
                    remote_port,
                    content: item,
                };
                Ok::<
                    ChannelMsg<Content>,
                    ChannelSendError,
                >(msg)
            }
        });

        ChannelSender {
            local_port,
            remote_port,
            sink: Box::pin(adapted_tx),
            drop_tx
        }
    }
}

impl<Item> fmt::Debug for ChannelSender<Item> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "ChannelSender {{local_port={}, remote_port={}}}",
               &self.local_port, &self.remote_port)
    }
}

impl<Item> Sink<Item> for ChannelSender<Item>
{
    type Error = ChannelSendError;
    fn poll_ready(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Result<(), Self::Error>> {
        let this = self.project();
        this.sink.poll_ready(cx)
    }
    fn start_send(self: Pin<&mut Self>, item: Item) -> Result<(), Self::Error> {
        let this = self.project();
        this.sink.start_send(item)
    }
    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Result<(), Self::Error>> {
        let this = self.project();
        this.sink.poll_flush(cx)
    }
    fn poll_close(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Result<(), Self::Error>> {
        let this = self.project();
        this.sink.poll_close(cx)
    }
}

#[pinned_drop]
impl<Item> PinnedDrop
for ChannelSender<Item> {
    fn drop(self: Pin<&mut Self>) {
        let mut this = self.project();
        block_on(async move {
            let _ = this.drop_tx.send(ChannelMsg::SenderDropped {local_port: *this.local_port}).await;
        });
    }
}
