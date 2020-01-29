use futures::channel::{mpsc};
use futures::future;
use futures::sink::{SinkExt};
use futures::stream::{self, Stream, StreamExt};
use futures::task::{Context, Poll};
use futures::lock::Mutex;
use futures::ready;
use pin_project::{pin_project, pinned_drop};
use std::fmt;
use std::pin::Pin;
use std::sync::{Arc};
use std::error::Error;
use async_thread::on_thread;
use async_trait::async_trait;
use log::trace;

use crate::codec::Deserializer;
use crate::receive_buffer::{ChannelReceiverBufferDequeuer};
use crate::multiplexer::ChannelMsg;

/// An error occured during receiving a message.
#[derive(Debug)]
pub enum ReceiveError {
    /// The multiplexer encountered an error or was terminated.
    MultiplexerError,
    /// A deserialization error occured.
    DeserializationError (Box<dyn Error + Send + 'static>)
}

impl fmt::Display for ReceiveError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Self::MultiplexerError => write!(f, "A channel multiplexer error occured."),
            Self::DeserializationError (err) => 
                write!(f, "A deserialization error occured: {}", err)
        }
    }
}

impl Error for ReceiveError {}

#[pin_project(PinnedDrop)]
pub struct RawReceiver<Content> where Content: Send {
    local_port: u32,
    remote_port: u32,
    pub(crate) closed: bool,
    #[pin]
    stream: Pin<Box<dyn Stream<Item=Result<Content, ReceiveError>> + Send>>,
    #[pin]
    pub(crate) drop_tx: mpsc::Sender<ChannelMsg<Content>>,
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
    type Item = Result<Content, ReceiveError>;
    fn poll_next(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Option<Self::Item>> {
        let this = self.project();
        this.stream.poll_next(cx)        
    }
}

#[pinned_drop]
impl<Content> PinnedDrop for RawReceiver<Content> where Content: Send {
    fn drop(self: Pin<&mut Self>) {
        trace!("RawReceiver dropping...");
        let mut this = self.project();
        if !*this.closed {
            on_thread(async {
                let _ = this.drop_tx.send(ChannelMsg::ReceiverClosed {local_port: *this.local_port, gracefully: false}).await;
            });
        }
        trace!("RawReceiver dropped.");
    }
}

#[async_trait]
trait CloseableStream : Stream {
    async fn close(self: Pin<&mut Self>);
}

#[pin_project]
pub struct Receiver<Item> {
    #[pin]
    inner: Pin<Box<dyn CloseableStream<Item=Result<Item, ReceiveError>>>>
}

impl<Item> Receiver<Item> {
    pub(crate) fn new<Content>(receiver: RawReceiver<Content>, deserialzer: Box<dyn Deserializer<Item, Content>>) -> Receiver<Item>
    where Content: Send + 'static,
          Item: 'static
    {
        let inner = ReceiverInner {
            receiver,
            deserialzer,
        };
        Receiver {
            inner: Box::pin(inner)
        }
    }

    /// Prevents the remote endpoint from sending new messages into this channel while
    /// allowing in-flight messages to be received.    
    pub async fn close(&mut self) {
        self.inner.as_mut().close().await;
    }
}

impl<Item> Stream for Receiver<Item> {
    type Item = Result<Item, ReceiveError>;
    fn poll_next(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Option<Self::Item>> {
        self.project().inner.poll_next(cx)
    }
}

#[pin_project]
struct ReceiverInner<Item, Content> where Content: Send {
    #[pin]
    receiver: RawReceiver<Content>,
    #[pin]
    deserialzer: Box<dyn Deserializer<Item, Content>>,
}

impl<Item, Content> Stream for ReceiverInner<Item, Content> where
    Content: Send,
{
    type Item = Result<Item, ReceiveError>;
    fn poll_next(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Option<Self::Item>> {
        let this = self.project();
        let item =
            match ready!(this.receiver.poll_next(cx)) {
                Some(Ok(content)) => Some(this.deserialzer.deserialize(content).map_err(ReceiveError::DeserializationError)),
                Some(Err(err)) => Some(Err(ReceiveError::from(err))),
                None => None
            };
        Poll::Ready(item)
    }
}

#[async_trait]
impl<Item, Content> CloseableStream for ReceiverInner<Item, Content> where Content: Send + 'static {
    async fn close(self: Pin<&mut Self>) {
        self.project().receiver.close().await; 
    }
}
