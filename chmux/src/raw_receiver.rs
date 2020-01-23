use futures::channel::{mpsc};
use futures::future;
use futures::sink::{SinkExt};
use futures::stream::{self, Stream, StreamExt};
use futures::task::{Context, Poll};
use futures::lock::Mutex;
use pin_project::{pin_project, pinned_drop};
use std::fmt;
use std::pin::Pin;
use std::sync::{Arc};
use async_thread::on_thread;
use log::trace;

use crate::receive_buffer::{ChannelReceiverBufferDequeuer};
use crate::multiplexer::ChannelMsg;

#[derive(Debug, Clone)]
pub enum RawReceiveError {
    /// The multiplexer encountered an error or was terminated.
    MultiplexerError
}


#[pin_project(PinnedDrop)]
pub struct RawReceiver<Content> where Content: Send {
    local_port: u32,
    remote_port: u32,
    closed: bool,
    #[pin]
    stream: Pin<Box<dyn Stream<Item=Result<Content, RawReceiveError>> + Send>>,
    #[pin]
    drop_tx: mpsc::Sender<ChannelMsg<Content>>,
}

impl<Content> RawReceiver<Content> where Content: 'static + Send {
    pub(crate) fn new(
        local_port: u32,
        remote_port: u32,        
        tx: mpsc::Sender<ChannelMsg<Content>>,
        rx_buffer: ChannelReceiverBufferDequeuer<Content>,
    ) -> RawReceiver<Content> 
    {
        let rx_buffer = Arc::new(Mutex::new(rx_buffer));
        let stream =
            stream::repeat(()).then(move |_| {
                let rx_buffer = rx_buffer.clone();
                async move {
                    let mut rx_buffer = rx_buffer.lock().await;
                    rx_buffer.dequeue().await
            }}).take_while(|opt_item| future::ready(opt_item.is_some()))
            .map(|opt_item| opt_item.unwrap());

        RawReceiver {
            local_port,
            remote_port,
            closed: false,
            drop_tx: tx,
            stream: Box::pin(stream),
        }       
    }

    /// Prevents the remote endpoint from sending new messages into this channel while
    /// allowing in-flight messages to be received.
    pub async fn close(&mut self) {
        if !self.closed {
            let _ = self.drop_tx.send(ChannelMsg::ReceiverClosed {local_port: self.local_port, gracefully: true}).await;     
            self.closed = true;
        }
    }
}

impl<Content> fmt::Debug for RawReceiver<Content> where Content: Send {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "RawReceiver {{local_port={}, remote_port={}}}",
               &self.local_port, &self.remote_port)
    }
}

impl<Content> Stream for RawReceiver<Content> where Content: Send
{
    type Item = Result<Content, RawReceiveError>;
    fn poll_next(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Option<Self::Item>> {
        let this = self.project();
        this.stream.poll_next(cx)        
    }
}

#[pinned_drop]
impl<Content> PinnedDrop for RawReceiver<Content> where Content: Send {
    fn drop(self: Pin<&mut Self>) {
        trace!("ChannelReceiver dropping...");
        let mut this = self.project();
        if !*this.closed {
            on_thread(async {
                let _ = this.drop_tx.send(ChannelMsg::ReceiverClosed {local_port: *this.local_port, gracefully: false}).await;
            });
        }
        trace!("ChannelReceiver dropped.");
    }
}

