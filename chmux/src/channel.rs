use std::pin::Pin;
use futures::task::{Context, Poll};
use futures::sink::{Sink};
use futures::stream::{Stream};
use pin_project::{pin_project};

use crate::sender::Sender;
use crate::receiver::Receiver;

/// A bi-directional communication channel, implementing `Sink` and `Stream`.
/// 
/// Can be split into separate `Sender` and `Receiver`.
#[pin_project]
pub struct Channel<SinkItem, StreamItem> {
    #[pin]
    pub(crate) sender: Sender<SinkItem>,
    #[pin]
    pub(crate) receiver: Receiver<StreamItem>,
}

impl<SinkItem, StreamItem> Channel<SinkItem, StreamItem> {
    /// Creates a bi-directional channel by combining a `Sender` and `Receiver`.
    pub fn new(sender: Sender<SinkItem>, receiver: Receiver<StreamItem>) -> Channel<SinkItem, StreamItem> {
        Channel { 
            sender, receiver
        }
    }

    /// Splits the channel into a `Sender` and `Receiver`.
    pub fn split(self) -> (Sender<SinkItem>, Receiver<StreamItem>) {
        let Channel {sender, receiver} = self;
        (sender, receiver)
    }
}

impl<SinkItem, StreamItem> Sink<SinkItem> for Channel<SinkItem, StreamItem> 
{
    type Error = <Sender<SinkItem> as Sink<SinkItem>>::Error;
    fn poll_ready(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Result<(), Self::Error>> {
        self.project().sender.poll_ready(cx)
    }
    fn start_send(self: Pin<&mut Self>, item: SinkItem) -> Result<(), Self::Error> {
        self.project().sender.start_send(item)
    }
    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Result<(), Self::Error>> {
        self.project().sender.poll_flush(cx)
    }
    fn poll_close(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Result<(), Self::Error>> {
        self.project().sender.poll_close(cx)
    }
}

impl<SinkItem, StreamItem> Stream for Channel<SinkItem, StreamItem>
{
    type Item = <Receiver<StreamItem> as Stream>::Item;
    fn poll_next(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Option<Self::Item>> {
        self.project().receiver.poll_next(cx)
    }
}


