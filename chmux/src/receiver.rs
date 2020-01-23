use std::pin::Pin;
use std::marker::PhantomData;
use std::error::Error;
use pin_project::{pin_project};
use futures::task::{Context, Poll};
use futures::ready;
use futures::stream::Stream;

use crate::codec::Deserializer;
use crate::raw_receiver::{RawReceiver, RawReceiveError};


pub enum ReceiveError {
    MultiplexerError,
    DeserializationError (Box<dyn Error + 'static>)
}

impl From<RawReceiveError> for ReceiveError {
    fn from(err: RawReceiveError) -> Self {
        match err {
            RawReceiveError::MultiplexerError => Self::MultiplexerError,
        }
    }
}



#[pin_project]
pub struct Receiver<Item> {
    #[pin]
    inner: Pin<Box<dyn Stream<Item=Result<Item, ReceiveError>>>>
}

impl<Item> Receiver<Item> {
    pub(crate) fn new<Content, Deser>(receiver: RawReceiver<Content>, deserialzer: Deser) -> Receiver<Item>
    where Deser: Deserializer<Item, Content> + 'static,
        Content: Send + 'static,
        Item: 'static
    {
        let inner = ReceiverInner {
            receiver,
            deserialzer,
            _phantom_item: PhantomData
        };
        Receiver {
            inner: Box::pin(inner)
        }
    }
}



impl<Item> Stream for Receiver<Item> {
    type Item = Result<Item, ReceiveError>;
    fn poll_next(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Option<Self::Item>> {
        self.project().inner.poll_next(cx)
    }
}

#[pin_project]
struct ReceiverInner<Item, Content, Deser> where Content: Send {
    #[pin]
    receiver: RawReceiver<Content>,
    #[pin]
    deserialzer: Deser,
    _phantom_item: PhantomData<Item>,
}

impl<Item, Content, Deser> Stream for ReceiverInner<Item, Content, Deser> where
    Deser: Deserializer<Item, Content>,
    Content: Send,
{
    type Item = Result<Item, ReceiveError>;
    fn poll_next(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Option<Self::Item>> {
        let this = self.project();
        let item =
            match ready!(this.receiver.poll_next(cx)) {
                Some(Ok(content)) => Some(this.deserialzer.deserialize(&content).map_err(|err|
                    ReceiveError::DeserializationError(Box::new(err)))),
                Some(Err(err)) => Some(Err(ReceiveError::from(err))),
                None => None
            };
        Poll::Ready(item)
    }
}

