use async_thread::on_thread;
use futures::channel::mpsc;
use futures::sink::SinkExt;
use futures::stream::Stream;
use futures::task::{Context, Poll};
use pin_project::{pin_project, pinned_drop};
use serde::de::DeserializeOwned;
use serde::Serialize;
use std::error::Error;
use std::fmt;
use std::pin::Pin;

use crate::codec::{CodecFactory, Deserializer};
use crate::multiplexer::{ChannelData, ChannelMsg};
use crate::receiver::Receiver;
use crate::sender::Sender;

/// An multiplexer server error.
#[derive(Debug)]
pub enum ServerError {
    /// Error deserializing the service request.
    DeserializationError(Box<dyn Error + Send + 'static>),
}

impl fmt::Display for ServerError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Self::DeserializationError(err) => write!(f, "A deserialization error occured: {}", err),
        }
    }
}

impl Error for ServerError {}

/// A service request by the remote endpoint.
///
/// Drop the request to reject it.
pub struct RemoteConnectToServiceRequest<Content, Codec>
where
    Content: Send,
{
    channel_data: Option<ChannelData<Content>>,
    codec_factory: Codec,
}

impl<Content, Codec> fmt::Debug for RemoteConnectToServiceRequest<Content, Codec>
where
    Content: Send,
{
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "RemoteConnectToServiceRequest")?;
        if let Some(channel_data) = self.channel_data.as_ref() {
            write!(f, "{{local_port={}, remote_port={}}}", channel_data.local_port, channel_data.remote_port)?;
        }
        Ok(())
    }
}

impl<Content, Codec> RemoteConnectToServiceRequest<Content, Codec>
where
    Content: Serialize + DeserializeOwned + Send + 'static,
    Codec: CodecFactory<Content>,
{
    pub(crate) fn new(
        channel_data: ChannelData<Content>, codec_factory: &Codec,
    ) -> RemoteConnectToServiceRequest<Content, Codec> {
        RemoteConnectToServiceRequest { channel_data: Some(channel_data), codec_factory: codec_factory.clone() }
    }

    /// Accepts the service request and returns a pair of channel sender and receiver.
    pub async fn accept<SinkItem, StreamItem>(mut self) -> (Sender<SinkItem>, Receiver<StreamItem>)
    where
        SinkItem: Serialize + 'static,
        StreamItem: DeserializeOwned + 'static,
    {
        let mut channel_data = self.channel_data.take().unwrap();
        // If multiplexer has terminated, sender and receiver will return errors.
        let _ = channel_data.tx.send(ChannelMsg::Accepted { local_port: channel_data.local_port }).await;
        let (raw_sender, raw_receiver) = channel_data.instantiate();

        let serializer = self.codec_factory.serializer();
        let sender = Sender::new(raw_sender, serializer);
        let deserializer = self.codec_factory.deserializer();
        let receiver = Receiver::new(raw_receiver, deserializer);
        (sender, receiver)
    }
}

impl<Content, Codec> Drop for RemoteConnectToServiceRequest<Content, Codec>
where
    Content: Send,
{
    fn drop(&mut self) {
        if let Some(mut channel_data) = self.channel_data.take() {
            on_thread(async {
                let _ = channel_data.tx.send(ChannelMsg::Rejected { local_port: channel_data.local_port }).await;
            });
        }
    }
}

/// Multiplexer server.
///
/// Provides a stream of remote service requests.
#[pin_project(PinnedDrop)]
pub struct Server<Service, Content, Codec>
where
    Content: Send,
{
    #[pin]
    pub(crate) serve_rx: mpsc::Receiver<(Content, RemoteConnectToServiceRequest<Content, Codec>)>,
    pub(crate) drop_tx: Option<mpsc::Sender<()>>,
    deserializer: Box<dyn Deserializer<Service, Content>>,
}

impl<Service, Content, Codec> fmt::Debug for Server<Service, Content, Codec>
where
    Content: Send,
{
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "Server")
    }
}

impl<Service, Content, Codec> Server<Service, Content, Codec>
where
    Service: DeserializeOwned + 'static,
    Content: Serialize + DeserializeOwned + Send,
    Codec: CodecFactory<Content>,
{
    pub(crate) fn new(
        serve_rx: mpsc::Receiver<(Content, RemoteConnectToServiceRequest<Content, Codec>)>,
        drop_tx: mpsc::Sender<()>, codec_factory: &Codec,
    ) -> Server<Service, Content, Codec> {
        Server { serve_rx, drop_tx: Some(drop_tx), deserializer: codec_factory.deserializer() }
    }
}

#[pinned_drop]
impl<Service, Content, Codec> PinnedDrop for Server<Service, Content, Codec>
where
    Content: Send,
{
    fn drop(self: Pin<&mut Self>) {
        let this = self.project();
        let mut drop_tx = this.drop_tx.take().unwrap();
        on_thread(async move {
            let _ = drop_tx.send(()).await;
        })
    }
}

impl<Service, Content, Codec> Stream for Server<Service, Content, Codec>
where
    Service: DeserializeOwned,
    Content: Send,
{
    type Item = (Result<Service, ServerError>, RemoteConnectToServiceRequest<Content, Codec>);
    fn poll_next(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Option<Self::Item>> {
        let this = self.project();
        match this.serve_rx.poll_next(cx) {
            Poll::Pending => Poll::Pending,
            Poll::Ready(None) => Poll::Ready(None),
            Poll::Ready(Some((service, req))) => {
                let service = this.deserializer.deserialize(service).map_err(ServerError::DeserializationError);
                Poll::Ready(Some((service, req)))
            }
        }
    }
}
