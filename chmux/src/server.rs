use futures::channel::{mpsc};
use futures::stream::{Stream};
use futures::sink::{SinkExt};
use futures::task::{Context, Poll};
use std::fmt;
use std::pin::Pin;
use pin_project::{pin_project, pinned_drop};
use async_thread::on_thread;

use crate::raw_channel::RawChannel;
use crate::multiplexer::{ChannelData, ChannelMsg};

/// A service request by the remote endpoint.
/// If the request is dropped, it is automatically rejected.
pub struct RemoteConnectToServiceRequest<Content> where Content: Send {
    channel_data: Option<ChannelData<Content>>,
}

impl<Content> fmt::Debug for RemoteConnectToServiceRequest<Content> where Content: Send {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "RemoteConnectToServiceRequest")
    }
}

impl<Content> RemoteConnectToServiceRequest<Content> where Content: 'static  + Send {
    pub(crate) fn new(channel_data: ChannelData<Content>)
        -> RemoteConnectToServiceRequest<Content> 
    {
        RemoteConnectToServiceRequest {
            channel_data: Some(channel_data),
        }
    }

    /// Accepts the service request and returns a pair of channel sender and receiver.
    pub async fn accept(mut self) -> RawChannel<Content> {
        let mut channel_data = self.channel_data.take().unwrap();
        // If multiplexer has terminated, sender and receiver will return errors.
        let _ = channel_data.tx.send(ChannelMsg::Accepted {local_port: channel_data.local_port}).await;
        channel_data.instantiate()
    }

    /// Rejects the service request, optionally providing the specified reason to the remote endpoint.
    pub async fn reject(mut self, reason: Option<Content>) {
        let mut channel_data = self.channel_data.take().unwrap();
        let _ = channel_data.tx.send(ChannelMsg::Rejected {local_port: channel_data.local_port, reason}).await;
    }
}

impl<Content> Drop for RemoteConnectToServiceRequest<Content> where Content: Send {
    fn drop(&mut self) {
        if let Some(mut channel_data) = self.channel_data.take() {
            on_thread(async {
                let _ = channel_data.tx.send(ChannelMsg::Rejected {local_port: channel_data.local_port, reason: None}).await;
            });    
        }
    }
}

#[pin_project(PinnedDrop)]
pub struct MultiplexerServer<Content> where Content: Send {
    #[pin]
    pub(crate) serve_rx: mpsc::Receiver<(Content, RemoteConnectToServiceRequest<Content>)>,
    pub(crate) drop_tx: mpsc::Sender<()>,
}

impl<Content> fmt::Debug for MultiplexerServer<Content> where Content: Send {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "MultiplexerServer")
    }
}

#[pinned_drop]
impl<Content> PinnedDrop for MultiplexerServer<Content> where Content: Send {
    fn drop(self: Pin<&mut Self>) {
        let this = self.project();
        on_thread(async move {
            let _ = this.drop_tx.send(()).await;
        })
    }
}

impl<Content> Stream for MultiplexerServer<Content> where Content: Send {
    type Item = (Content, RemoteConnectToServiceRequest<Content>);
    fn poll_next(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Option<Self::Item>> {
        let this = self.project();
        this.serve_rx.poll_next(cx)
    }
}
