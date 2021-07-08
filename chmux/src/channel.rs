use futures::{
    sink::Sink,
    stream::Stream,
    task::{Context, Poll},
};
use pin_project::pin_project;
use serde::{de::DeserializeOwned, Serialize};
use std::pin::Pin;

use crate::{receiver::ReceiverStream, sender::SenderSink};

/// A bi-directional communication channel, implementing [Sink] and [Stream].
#[pin_project]
pub struct Channel<SinkItem, StreamItem>
where
    SinkItem: Serialize + 'static,
{
    #[pin]
    sender: SenderSink<SinkItem>,
    #[pin]
    receiver: ReceiverStream<StreamItem>,
}

impl<SinkItem, StreamItem> Channel<SinkItem, StreamItem>
where
    SinkItem: Serialize,
{
    /// Creates a bi-directional channel by combining a [SenderSink] and [ReceiverStream].
    pub fn new(
        sender: SenderSink<SinkItem>, receiver: ReceiverStream<StreamItem>,
    ) -> Channel<SinkItem, StreamItem> {
        Channel { sender, receiver }
    }

    /// Splits the channel into a [SenderSink] and [ReceiverStream].
    pub fn split(self) -> (SenderSink<SinkItem>, ReceiverStream<StreamItem>) {
        let Channel { sender, receiver } = self;
        (sender, receiver)
    }
}

impl<SinkItem, StreamItem> Sink<SinkItem> for Channel<SinkItem, StreamItem>
where
    SinkItem: Serialize,
{
    type Error = <SenderSink<SinkItem> as Sink<SinkItem>>::Error;
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
where
    SinkItem: Serialize,
    StreamItem: DeserializeOwned,
{
    type Item = <ReceiverStream<StreamItem> as Stream>::Item;
    fn poll_next(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Option<Self::Item>> {
        self.project().receiver.poll_next(cx)
    }
}
