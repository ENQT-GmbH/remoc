use futures::channel::{mpsc};
use futures::future;
use futures::executor::block_on;
use futures::sink::{SinkExt};
use futures::stream::{self, Stream, StreamExt};
use futures::task::{Context, Poll};
use pin_project::{pin_project, pinned_drop};
use std::fmt;
use std::pin::Pin;
use std::sync::{Arc, Mutex};

use crate::receive_buffer::{ChannelReceiverBufferDequeuer};
use crate::multiplexer::ChannelMsg;

#[derive(Debug, Clone)]
pub enum ChannelReceiveError {
    /// The multiplexer encountered an error or was terminated.
    MultiplexerError
}


#[pin_project]
pub struct ChannelReceiver<Content> {
    local_port: u32,
    remote_port: u32,
    closed: bool,
    #[pin]
    stream: Pin<Box<dyn Stream<Item=Result<Content, ChannelReceiveError>>>>,
    #[pin]
    drop_tx: mpsc::Sender<ChannelMsg<Content>>,
}

impl<Content> ChannelReceiver<Content> {
    pub(crate) fn new(
        local_port: u32,
        remote_port: u32,        
        tx: mpsc::Sender<ChannelMsg<Content>>,
        rx_buffer: ChannelReceiverBufferDequeuer<Content>,
    ) -> ChannelReceiver<Content> 
    where Content: 'static {
        let rx_buffer = Arc::new(Mutex::new(rx_buffer));
        let stream =
            stream::repeat(()).then(move |_| {
                let rx_buffer = rx_buffer.clone();
                async move {
                    let mut rx_buffer = rx_buffer.lock().unwrap();
                    rx_buffer.dequeue().await
            }}).take_while(|opt_item| future::ready(opt_item.is_some()))
            .map(|opt_item| opt_item.unwrap());

        ChannelReceiver {
            local_port,
            remote_port,
            closed: false,
            drop_tx: tx,
            stream: Box::pin(stream),
        }       
    }

    /// Prevents the remote endpoint from sending new messages into this channel while
    /// allowing in-flight messages to be received.
    pub fn close(&mut self) {
        if !self.closed {
            block_on(async {
                let _ = self.drop_tx.send(ChannelMsg::ReceiverClosed {local_port: self.local_port, gracefully: true}).await;
            });        
            self.closed = true;
        }
    }
}

impl<Item> fmt::Debug for ChannelReceiver<Item> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "ChannelReceiver {{local_port={}, remote_port={}}}",
               &self.local_port, &self.remote_port)
    }
}

impl<Item> Stream for ChannelReceiver<Item>
{
    type Item = Result<Item, ChannelReceiveError>;
    fn poll_next(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Option<Self::Item>> {
        let this = self.project();
        this.stream.poll_next(cx)        
    }
}

#[pinned_drop]
impl<Item> PinnedDrop for ChannelReceiver<Item> {
    fn drop(self: Pin<&mut Self>) {
        let mut this = self.project();
        if !*this.closed {
            block_on(async {
                let _ = this.drop_tx.send(ChannelMsg::ReceiverClosed {local_port: *this.local_port, gracefully: false}).await;
            });
        }
    }
}

