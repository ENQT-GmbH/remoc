use futures::channel::{mpsc};
use futures::sink::{Sink, SinkExt};
use futures::task::{Context, Poll};
use pin_project::{pin_project, pinned_drop};
use std::fmt;
use std::pin::Pin;
use std::sync::{Arc};
use async_thread::on_thread;
use log::trace;

use crate::send_lock::{ChannelSendLockRequester};
use crate::multiplexer::ChannelMsg;

#[derive(Debug, Clone)]
pub enum RawSendError {
    /// Other side closed receiving end of channel.
    Closed { 
        /// True if other side still processes items send up until now.
        gracefully: bool,
    },
    /// The multiplexer encountered an error or was terminated.
    MultiplexerError
}

impl From<mpsc::SendError> for RawSendError
{
    fn from(err: mpsc::SendError) -> Self {
        if err.is_disconnected() {
            return Self::MultiplexerError;
        }
        panic!("Error sending data to multiplexer for unknown reasons.");
    }
}

#[pin_project(PinnedDrop)]
pub struct RawSender<Content> where Content: Send {
    pub(crate) local_port: u32,
    pub(crate) remote_port: u32,
    #[pin]
    sink: Pin<Box<
        dyn Sink<
            Content,
            Error = RawSendError,
        > + Send,
    >>,
    #[pin]
    drop_tx: mpsc::Sender<ChannelMsg<Content>>,
}

impl<Content> RawSender<Content> where Content: Send
{
    pub(crate) fn new(
        local_port: u32,
        remote_port: u32,
        tx: mpsc::Sender<ChannelMsg<Content>>,
        tx_lock: ChannelSendLockRequester,
    ) -> RawSender<Content>
    where
        Content: 'static + Send,
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
                    RawSendError,
                >(msg)
            }
        });

        RawSender {
            local_port,
            remote_port,
            sink: Box::pin(adapted_tx),
            drop_tx
        }
    }
}

impl<Content> fmt::Debug for RawSender<Content> where Content: Send {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "RawSender {{local_port={}, remote_port={}}}",
               &self.local_port, &self.remote_port)
    }
}

impl<Content> Sink<Content> for RawSender<Content> where Content: Send
{
    type Error = RawSendError;
    fn poll_ready(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Result<(), Self::Error>> {
        let this = self.project();
        this.sink.poll_ready(cx)
    }
    fn start_send(self: Pin<&mut Self>, item: Content) -> Result<(), Self::Error> {
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
impl<Content> PinnedDrop for RawSender<Content> where Content: Send {
    fn drop(self: Pin<&mut Self>) {
        trace!("ChannelSender dropping...");
        let mut this = self.project();
        on_thread(async move {
            let _ = this.drop_tx.send(ChannelMsg::SenderDropped {local_port: *this.local_port}).await;
        });
        trace!("ChannelSender dropped.");
    }
}
