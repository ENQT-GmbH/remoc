use std::fmt;
use std::pin::Pin;
use pin_project::{pin_project};
use futures::task::{Context, Poll};
use futures::sink::{Sink};
use futures::stream::{Stream};

use crate::codec;
use crate::raw_sender::RawSender;
use crate::raw_receiver::RawReceiver;
use crate::channel::Channel;
use crate::sender::Sender;
use crate::receiver::Receiver;

/// A bi-direction raw communication channel.
#[pin_project]
pub struct RawChannel<Content> where Content: Send {
    #[pin]
    pub(crate) sender: RawSender<Content>,
    #[pin]
    pub(crate) receiver: RawReceiver<Content>
}

impl<Content> fmt::Debug for RawChannel<Content> where Content: Send {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "RawChannel {{local_port={}, remote_port={}}}",
               &self.sender.local_port, &self.sender.remote_port)
    }
}

impl<Content> RawChannel<Content> where Content: Send {
    /// Splits the raw channel into a `RawSender` and `RawReceiver`.
    pub fn split(self) -> (RawSender<Content>, RawReceiver<Content>) {
        let RawChannel {sender, receiver} = self;
        (sender, receiver)
    }

    /// Transforms items transferred over this channel using the specified codec.
    pub fn codec<SinkItem, StreamItem, Codec>(self, codec: Codec) -> Channel<SinkItem, StreamItem> 
    where Codec: codec::Codec<SinkItem, StreamItem, Content>,
        SinkItem: 'static, StreamItem: 'static,
        <Codec as codec::Codec<SinkItem, StreamItem, Content>>::Serializer: 'static,
        <Codec as codec::Codec<SinkItem, StreamItem, Content>>::Deserializer: 'static,
        Content: 'static,
    {        
        let RawChannel {sender: raw_sender, receiver: raw_receiver} = self;
        let (serializer, deserializer) = codec.split();
        let sender = Sender::new(raw_sender, serializer);
        let receiver = Receiver::new(raw_receiver, deserializer);
        Channel {sender, receiver}
    }
}

impl<Content> Sink<Content> for RawChannel<Content> where Content: Send
{
    type Error = <RawSender<Content> as Sink<Content>>::Error;
    fn poll_ready(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Result<(), Self::Error>> {
        self.project().sender.poll_ready(cx)
    }
    fn start_send(self: Pin<&mut Self>, item: Content) -> Result<(), Self::Error> {
        self.project().sender.start_send(item)
    }
    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Result<(), Self::Error>> {
        self.project().sender.poll_flush(cx)
    }
    fn poll_close(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Result<(), Self::Error>> {
        self.project().sender.poll_close(cx)
    }
}

impl<Content> Stream for RawChannel<Content> where Content: Send
{
    type Item = <RawReceiver<Content> as Stream>::Item;
    fn poll_next(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Option<Self::Item>> {
        self.project().receiver.poll_next(cx)
    }
}

