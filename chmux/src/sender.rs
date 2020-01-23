use std::pin::Pin;
use std::marker::PhantomData;
use std::error::Error;
use pin_project::{pin_project};
use futures::task::{Context, Poll};
use futures::ready;
use futures::sink::Sink;

use crate::raw_sender::{RawSender, RawSendError};
use crate::codec::Serializer;

pub enum SendError {
    Closed { gracefully: bool},
    MultiplexerError,
    SerializationError (Box<dyn Error + 'static>)
}

impl From<RawSendError> for SendError {
    fn from(err: RawSendError) -> Self {
        match err {
            RawSendError::Closed {gracefully} => Self::Closed {gracefully},
            RawSendError::MultiplexerError => Self::MultiplexerError,
        }
    }
}


#[pin_project]
pub struct Sender<Item> {
    #[pin]
    inner: Pin<Box<dyn Sink<Item, Error=SendError>>>
}

impl<Item> Sender<Item> {
    pub(crate) fn new<Content, Ser>(sender: RawSender<Content>, serializer: Ser) -> Sender<Item>
    where Ser: Serializer<Item, Content> + 'static, Content: Send + 'static, Item: 'static
    {
        let inner = SenderInner {
            sender,
            serializer,
            _phantom_item: PhantomData
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
struct SenderInner<Item, Content, Ser> where Content: Send {
    #[pin]
    sender: RawSender<Content>,
    #[pin]
    serializer: Ser,
    _phantom_item: PhantomData<Item>,
}

impl<Item, Content, Ser> Sink<Item> for SenderInner<Item, Content, Ser>
where
    Ser: Serializer<Item, Content>,
    Content: Send
{
    type Error = SendError;

    fn poll_ready(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.project().sender.poll_ready(cx).map_err(SendError::from)
    }

    fn start_send(mut self: Pin<&mut Self>, item: Item) -> Result<(), Self::Error> {
        let content = self.as_mut().project().serializer.serialize(&item).map_err(|err|
            SendError::SerializationError(Box::new(err)))?;
        self.as_mut().project().sender.start_send(content)?;
        Ok(())
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.project().sender.poll_flush(cx).map_err(SendError::from)
    }

    fn poll_close(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        ready!(self.as_mut().poll_flush(cx))?;
        self.project().sender.poll_close(cx).map_err(SendError::from)
    }
}