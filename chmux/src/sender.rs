use futures::{
    channel::{mpsc, oneshot},
    lock::Mutex,
    sink::{Sink, SinkExt},
    task::{Context, Poll},
    Future, FutureExt,
};
use pin_project::{pin_project, pinned_drop};
use serde::Serialize;
use std::{
    error::Error,
    fmt,
    pin::Pin,
    sync::{Arc, Weak},
};

use crate::{codec::Serializer, multiplexer::ChannelMsg, send_lock::ChannelSendLockRequester};

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
    SerializationError(Box<dyn Error + Send + 'static>),
}

impl SendError {
    /// Returns true, if error is due to channel being closed or multiplexer
    /// being terminated.
    pub fn is_terminated(&self) -> bool {
        match self {
            Self::Closed { .. } | Self::MultiplexerError => true,
            Self::SerializationError(_) => false,
        }
    }
}

impl fmt::Display for SendError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Self::Closed { gracefully } if *gracefully => {
                write!(f, "Remote endpoint does not want any more data to be sent.")
            }
            Self::Closed { .. } => write!(f, "Remote endpoint closed its receiver."),
            Self::MultiplexerError => write!(f, "A multiplexer error has occured or it has been terminated."),
            Self::SerializationError(err) => write!(f, "A serialization error occured: {}", err),
        }
    }
}

impl Error for SendError {}

impl From<mpsc::SendError> for SendError {
    fn from(err: mpsc::SendError) -> Self {
        if err.is_disconnected() {
            return Self::MultiplexerError;
        }
        panic!("Error sending data to multiplexer for unknown reasons.");
    }
}

#[pin_project(PinnedDrop)]
pub struct RawSender<Content>
where
    Content: Send,
{
    pub(crate) local_port: u32,
    pub(crate) remote_port: u32,
    #[pin]
    sink: Pin<Box<dyn Sink<Content, Error = SendError> + Send>>,
    #[pin]
    tx: mpsc::Sender<ChannelMsg<Content>>,
    hangup_notify: Weak<Mutex<Option<Vec<oneshot::Sender<()>>>>>,
}

impl<Content> RawSender<Content>
where
    Content: Send,
{
    pub(crate) fn new(
        local_port: u32, remote_port: u32, tx: mpsc::Sender<ChannelMsg<Content>>,
        tx_lock: ChannelSendLockRequester, hangup_notify: Weak<Mutex<Option<Vec<oneshot::Sender<()>>>>>,
    ) -> RawSender<Content>
    where
        Content: 'static + Send,
    {
        let control_tx = tx.clone();
        let tx_lock = Arc::new(futures::lock::Mutex::new(tx_lock));
        let adapted_tx = tx.with(move |item: Content| {
            let tx_lock = tx_lock.clone();
            async move {
                let mut tx_lock = tx_lock.lock().await;
                tx_lock.request().await?;
                let msg = ChannelMsg::SendMsg { remote_port, content: item };
                Ok::<ChannelMsg<Content>, SendError>(msg)
            }
        });

        RawSender { local_port, remote_port, sink: Box::pin(adapted_tx), tx: control_tx, hangup_notify }
    }
}

impl<Content> Sink<Content> for RawSender<Content>
where
    Content: Send,
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
impl<Content> PinnedDrop for RawSender<Content>
where
    Content: Send,
{
    fn drop(self: Pin<&mut Self>) {
        // required for correct drop order
    }
}

/// This Future resolves when the remote endpoint has closed its receiver.
///
/// It will also resolve when the channel is closed or the channel multiplexer
/// is shutdown.
pub struct HangupNotify {
    fut: Pin<Box<dyn Future<Output = ()> + Send>>,
}

impl Future for HangupNotify {
    type Output = ();
    fn poll(mut self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
        self.fut.as_mut().poll(cx)
    }
}

/// Send end of a multiplexer channel.
///
/// Implements a `Sink`.
#[pin_project]
pub struct Sender<Item> {
    local_port: u32,
    remote_port: u32,
    #[pin]
    inner: Pin<Box<dyn Sink<Item, Error = SendError> + Send>>,
    hangup_notify: Weak<Mutex<Option<Vec<oneshot::Sender<()>>>>>,
}

impl<Item> Sender<Item>
where
    Item: Serialize,
{
    pub(crate) fn new<Content>(
        sender: RawSender<Content>, serializer: Box<dyn Serializer<Item, Content>>,
    ) -> Sender<Item>
    where
        Content: Send + 'static,
        Item: 'static,
    {
        let local_port = sender.local_port;
        let remote_port = sender.remote_port;
        let hangup_notify = sender.hangup_notify.clone();
        let inner = SenderInner { sender, serializer };
        Sender { local_port, remote_port, inner: Box::pin(inner), hangup_notify }
    }

    /// Returns a Future that will resolve when the remote endpoint has closed its receiver.
    ///
    /// It will resolve immediately when the remote endpoint has already hung up.
    pub fn hangup_notify(&self) -> HangupNotify {
        let hangup_notify = self.hangup_notify.clone();
        let fut = async move {
            let (tx, rx) = oneshot::channel();
            if let Some(hangup_notify) = hangup_notify.upgrade() {
                if let Some(notifiers) = hangup_notify.lock().await.as_mut() {
                    notifiers.push(tx);
                } else {
                    drop(tx);
                }
            } else {
                drop(tx);
            }
            let _ = rx.await;
        };
        HangupNotify { fut: fut.boxed() }
    }
}

impl<Item> fmt::Debug for Sender<Item> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "Sender {{local_port={}, remote_port={}}}", self.local_port, self.remote_port)
    }
}

impl<Item> Sink<Item> for Sender<Item>
where
    Item: Serialize,
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
struct SenderInner<Item, Content>
where
    Content: Send,
{
    #[pin]
    sender: RawSender<Content>,
    #[pin]
    serializer: Box<dyn Serializer<Item, Content>>,
}

impl<Item, Content> Sink<Item> for SenderInner<Item, Content>
where
    Item: Serialize,
    Content: Send,
{
    type Error = SendError;

    fn poll_ready(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.project().sender.poll_ready(cx).map_err(SendError::from)
    }

    fn start_send(mut self: Pin<&mut Self>, item: Item) -> Result<(), Self::Error> {
        let content =
            self.as_mut().project().serializer.serialize(item).map_err(SendError::SerializationError)?;
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
