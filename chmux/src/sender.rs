use futures::channel::{mpsc};
use futures::sink::{Sink, SinkExt};
use futures::task::{Context, Poll};
use pin_project::{pin_project, pinned_drop};
use std::fmt;
use std::pin::Pin;
use std::sync::{Arc};
use std::error::Error;
use async_thread::on_thread;
use log::trace;

use crate::send_lock::{ChannelSendLockRequester};
use crate::multiplexer::ChannelMsg;
use crate::codec::Serializer;

/// An error occured during sending of a message.
#[derive(Debug)]
pub enum SendError {
    /// Other side closed receiving end of channel.
    Closed { 
        /// True if other side still processes items send up until now.
        gracefully: bool,
    },
    /// The multiplexer encountered an error or was terminated.
    MultiplexerError,
    /// A serialization error occured.
    SerializationError (Box<dyn Error + Send + 'static>),
}

impl fmt::Display for SendError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Self::Closed {gracefully} if *gracefully => 
                write!(f, "Remote endpoint does not want any more data to be sent."),
            Self::Closed {..} => write!(f, "Remote endpoint closed its receiver."),
            Self::MultiplexerError => write!(f, "A channel multiplexer error occured."),
            Self::SerializationError (err) => 
                write!(f, "A serialization error occured: {}", err)
        }
    }
}

impl Error for SendError {}

impl From<mpsc::SendError> for SendError
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
            Error = SendError,
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
                    SendError,
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


impl<Content> Sink<Content> for RawSender<Content> where Content: Send
{
    type Error = SendError;
    fn poll_ready(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Result<(), Self::Error>> {
        self.project().sink.poll_ready(cx)
    }
    fn start_send(self: Pin<&mut Self>, item: Content) -> Result<(), Self::Error> {
        self.project().sink.start_send(item)
    }
    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Result<(), Self::Error>> {
        self.project().sink.poll_flush(cx)
    }
    fn poll_close(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Result<(), Self::Error>> {
        self.project().sink.poll_close(cx)
    }
}

#[pinned_drop]
impl<Content> PinnedDrop for RawSender<Content> where Content: Send {
    fn drop(self: Pin<&mut Self>) {
        trace!("RawSender dropping...");
        let mut this = self.project();
        on_thread(async move {
            let _ = this.drop_tx.send(ChannelMsg::SenderDropped {local_port: *this.local_port}).await;
        });
        trace!("RawSender dropped.");
    }
}


#[pin_project]
pub struct Sender<Item> {
    #[pin]
    inner: Pin<Box<dyn Sink<Item, Error=SendError>>>
}

impl<Item> Sender<Item> {
    pub(crate) fn new<Content>(sender: RawSender<Content>, serializer: Box<dyn Serializer<Item, Content>>) -> Sender<Item>
    where Content: Send + 'static, Item: 'static
    { 
        let inner = SenderInner {
            sender,
            serializer,
        };
        Sender {
            inner: Box::pin(inner)
        }
    }
}


impl<Item> Sink<Item> for Sender<Item>
{
    type Error = SendError;
    fn poll_ready(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.project().inner.poll_ready(cx)
    }
    fn start_send(self: Pin<&mut Self>, item: Item) -> Result<(), Self::Error> {
        self.project().inner.start_send(item)
    }
    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.project().inner.poll_flush(cx)
    }
    fn poll_close(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.project().inner.poll_close(cx)
    }
}

#[pin_project]
struct SenderInner<Item, Content> where Content: Send {
    #[pin]
    sender: RawSender<Content>,
    #[pin]
    serializer: Box<dyn Serializer<Item, Content>>,
}

impl<Item, Content> Sink<Item> for SenderInner<Item, Content>
where
    Content: Send
{
    type Error = SendError;

    fn poll_ready(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.project().sender.poll_ready(cx).map_err(SendError::from)
    }

    fn start_send(mut self: Pin<&mut Self>, item: Item) -> Result<(), Self::Error> {
        let content = self.as_mut().project().serializer.serialize(item).map_err(SendError::SerializationError)?;
        self.as_mut().project().sender.start_send(content)?;
        Ok(())
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.project().sender.poll_flush(cx).map_err(SendError::from)
    }

    fn poll_close(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.project().sender.poll_close(cx).map_err(SendError::from)
    }
}

