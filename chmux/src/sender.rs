use bytes::Bytes;
use futures::{
    future::{BoxFuture, LocalBoxFuture},
    ready,
    sink::Sink,
    task::{Context, Poll},
    Future, FutureExt,
};
use serde::Serialize;
use std::{
    error::Error,
    fmt,
    mem::size_of,
    pin::Pin,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Weak,
    },
};
use tokio::sync::{mpsc, oneshot, Mutex};

use crate::{
    client::ConnectResponse,
    codec::Serializer,
    credit::{AssignedCredits, CreditUser},
    multiplexer::PortEvt,
    Connect, ConnectError, PortNumber,
};

/// An error occured during sending of a message.
#[derive(Debug)]
pub enum SendError {
    /// The multiplexer terminated.
    Multiplexer,
    /// Other side closed receiving end of channel.
    Closed {
        /// True, if remote endpoint still processes messages that were already sent.
        gracefully: bool,
    },
    /// This side has been closed.
    SinkClosed,
    /// A serialization error occured.
    SerializationError(Box<dyn Error + Send + Sync + 'static>),
    /// Data exceeds maximum size.
    ExceedsMaxDataSize {
        /// Actual data size.
        data_size: usize,
        /// Maximum allowed data size.
        max_size: usize,
    },
    /// Port count exceeds limit.
    ExceedsMaxPortCount {
        /// Actual port count.
        port_count: usize,
        /// Maximum allowed port count.
        max_count: usize,
    },
}

impl SendError {
    /// Returns true, if error is due to channel being closed or multiplexer
    /// being terminated.
    pub fn is_terminated(&self) -> bool {
        matches!(self, Self::Closed { .. } | Self::Multiplexer)
    }
}

impl fmt::Display for SendError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Self::Multiplexer => write!(f, "multiplexer terminated"),
            Self::Closed { gracefully } => write!(
                f,
                "remote endpoint closed channel{}",
                if *gracefully { " but still processes sent messages" } else { "" }
            ),
            Self::SinkClosed => write!(f, "sink has been closed"),
            Self::SerializationError(err) => write!(f, "serialization error: {}", err),
            Self::ExceedsMaxDataSize { data_size, max_size } => {
                write!(f, "data ({} bytes) exceeds maximum allowed size ({} bytes)", data_size, max_size)
            }
            Self::ExceedsMaxPortCount { port_count, max_count } => {
                write!(f, "port count ({}) exceeds maximum allowed port count ({})", port_count, max_count)
            }
        }
    }
}

impl Error for SendError {}

impl<T> From<mpsc::error::SendError<T>> for SendError {
    fn from(_err: mpsc::error::SendError<T>) -> Self {
        Self::Multiplexer
    }
}

/// An error occured during sending of a message.
#[derive(Debug)]
pub enum TrySendError {
    /// Channel queue is full.
    ///
    /// Sending should be retried.
    Full,
    /// Send error.
    Send(SendError),
}

impl fmt::Display for TrySendError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Self::Full => write!(f, "channel queue is full"),
            Self::Send(err) => write!(f, "{}", err),
        }
    }
}

impl From<SendError> for TrySendError {
    fn from(err: SendError) -> Self {
        Self::Send(err)
    }
}

impl From<mpsc::error::TrySendError<PortEvt>> for TrySendError {
    fn from(err: mpsc::error::TrySendError<PortEvt>) -> Self {
        match err {
            mpsc::error::TrySendError::Full(_) => Self::Full,
            mpsc::error::TrySendError::Closed(_) => Self::Send(SendError::Multiplexer),
        }
    }
}

impl Error for TrySendError {}

/// This future resolves when the remote endpoint has closed its receiver.
///
/// It will also resolve when the channel is closed or the channel multiplexer
/// is shutdown.
pub struct Closed {
    fut: Pin<Box<dyn Future<Output = ()> + Send>>,
}

impl Closed {
    fn new(hangup_notify: &Weak<Mutex<Option<Vec<oneshot::Sender<()>>>>>) -> Self {
        let hangup_notify = hangup_notify.clone();
        let fut = async move {
            if let Some(hangup_notify) = hangup_notify.upgrade() {
                if let Some(notifiers) = hangup_notify.lock().await.as_mut() {
                    let (tx, rx) = oneshot::channel();
                    notifiers.push(tx);
                    let _ = rx.await;
                }
            }
        };
        Self { fut: fut.boxed() }
    }
}

impl Future for Closed {
    type Output = ();
    fn poll(mut self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
        self.fut.as_mut().poll(cx)
    }
}

/// Sends byte data over a channel.
pub struct RawSender {
    local_port: u32,
    remote_port: u32,
    max_data_size: usize,
    chunk_size: usize,
    tx: mpsc::Sender<PortEvt>,
    credits: CreditUser,
    hangup_recved: Weak<AtomicBool>,
    hangup_notify: Weak<Mutex<Option<Vec<oneshot::Sender<()>>>>>,
    _drop_tx: oneshot::Sender<()>,
}

impl fmt::Debug for RawSender {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("RawSender")
            .field("local_port", &self.local_port)
            .field("remote_port", &self.remote_port)
            .field("is_closed", &self.is_closed())
            .finish()
    }
}

impl RawSender {
    /// Create a new raw sender.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        local_port: u32, remote_port: u32, max_data_size: usize, chunk_size: usize, tx: mpsc::Sender<PortEvt>,
        credits: CreditUser, hangup_recved: Weak<AtomicBool>,
        hangup_notify: Weak<Mutex<Option<Vec<oneshot::Sender<()>>>>>,
    ) -> Self {
        let (_drop_tx, drop_rx) = oneshot::channel();
        let tx_drop = tx.clone();
        tokio::spawn(async move {
            let _ = drop_rx.await;
            let _ = tx_drop.send(PortEvt::SenderDropped { local_port }).await;
        });

        Self {
            local_port,
            remote_port,
            max_data_size,
            chunk_size,
            tx,
            credits,
            hangup_recved,
            hangup_notify,
            _drop_tx,
        }
    }

    /// Number of chunks to send data of given size.
    fn chunks(&self, size: usize) -> usize {
        let mut n = size / self.chunk_size;
        if size % self.chunk_size > 0 {
            n += 1;
        }
        n
    }

    /// The local port number.
    pub fn local_port(&self) -> u32 {
        self.local_port
    }

    /// The remote port number.
    pub fn remote_port(&self) -> u32 {
        self.remote_port
    }

    /// Maximum allowed size of data to be sent in one request.
    pub fn max_data_size(&self) -> usize {
        self.max_data_size
    }

    /// Maximum number of ports that can be sent in one request.
    pub fn max_port_count(&self) -> usize {
        (self.max_data_size / size_of::<u32>()).max(1)
    }

    /// Sends data over the channel.
    ///
    /// Waits until send space becomes available.
    /// Data is transmitted in chunks if it exceeds the maximum chunk size.
    ///
    /// # Cancel safety
    /// If this function is cancelled before completion, the remote endpoint will receive no data.
    pub async fn send(&mut self, mut data: Bytes) -> Result<(), SendError> {
        if data.len() > self.max_data_size {
            return Err(SendError::ExceedsMaxDataSize { data_size: data.len(), max_size: self.max_data_size });
        }

        if data.is_empty() {
            let mut credits = self.credits.request(1).await?;
            let msg = PortEvt::SendData {
                remote_port: self.remote_port,
                data,
                first: true,
                last: true,
                credit: credits.take_one(),
            };
            self.tx.send(msg).await?;
        } else {
            let mut first = true;
            let mut credits = AssignedCredits::default();
            while !data.is_empty() {
                if credits.is_empty() {
                    credits = self.credits.request(self.chunks(data.len()).min(u16::MAX as usize) as u16).await?;
                }

                let at = data.len().min(self.chunk_size);
                let chunk = data.split_to(at);
                let msg = PortEvt::SendData {
                    remote_port: self.remote_port,
                    data: chunk,
                    first,
                    last: data.is_empty(),
                    credit: credits.take_one(),
                };
                self.tx.send(msg).await?;
                first = false;
            }
        }

        Ok(())
    }

    /// Tries to send data over the channel.
    ///
    /// Does not wait until send space becomes available.
    /// The maximum size of data sendable by this function is limited by
    /// the total receive buffer size.
    pub fn try_send(&mut self, data: &Bytes) -> Result<(), TrySendError> {
        let mut data = data.clone();

        if data.len() > self.max_data_size {
            return Err(
                SendError::ExceedsMaxDataSize { data_size: data.len(), max_size: self.max_data_size }.into()
            );
        }

        if data.is_empty() {
            match self.credits.try_request(1)? {
                Some(mut credits) => {
                    let msg = PortEvt::SendData {
                        remote_port: self.remote_port,
                        data,
                        first: true,
                        last: true,
                        credit: credits.take_one(),
                    };
                    self.tx.try_send(msg)?;
                    Ok(())
                }
                None => Err(TrySendError::Full),
            }
        } else {
            match self.credits.try_request(self.chunks(data.len()).min(u16::MAX as usize) as u16)? {
                Some(mut credits) => {
                    let mut first = true;
                    while !data.is_empty() {
                        let at = data.len().min(self.chunk_size);
                        let chunk = data.split_to(at);
                        let msg = PortEvt::SendData {
                            remote_port: self.remote_port,
                            data: chunk,
                            first,
                            last: data.is_empty(),
                            credit: credits.take_one(),
                        };
                        self.tx.try_send(msg)?;
                        first = false;
                    }
                    Ok(())
                }
                None => Err(TrySendError::Full),
            }
        }
    }

    /// Connect to new ports over this port.
    pub async fn connect(&mut self, ports: Vec<PortNumber>, wait: bool) -> Result<Vec<Connect>, SendError> {
        if ports.len() > self.max_port_count() {
            return Err(SendError::ExceedsMaxPortCount {
                port_count: ports.len(),
                max_count: self.max_port_count(),
            });
        }

        let mut credits = self.credits.request(1).await?;

        let mut ports_response = Vec::new();
        let mut sent_txs = Vec::new();
        let mut connects = Vec::new();

        for port in ports {
            let (response_tx, response_rx) = oneshot::channel();
            ports_response.push((port, response_tx));

            let response = tokio::spawn(async move {
                match response_rx.await {
                    Ok(ConnectResponse::Accepted(sender, receiver)) => Ok((sender, receiver)),
                    Ok(ConnectResponse::Rejected { no_ports }) => {
                        if no_ports {
                            Err(ConnectError::RemotePortsExhausted)
                        } else {
                            Err(ConnectError::Rejected)
                        }
                    }
                    Err(_) => Err(ConnectError::MultiplexerError),
                }
            });

            let (sent_tx, sent_rx) = mpsc::channel(1);
            sent_txs.push(sent_tx);

            connects.push(Connect { sent_rx, response });
        }

        let msg = PortEvt::SendPorts {
            remote_port: self.remote_port,
            credit: credits.take_one(),
            wait,
            ports: ports_response,
        };
        self.tx.send(msg).await?;

        Ok(connects)
    }

    /// True, once the remote endpoint has closed its receiver.
    pub fn is_closed(&self) -> bool {
        self.hangup_recved.upgrade().map(|hr| hr.load(Ordering::SeqCst)).unwrap_or_default()
    }

    /// Returns a future that will resolve when the remote endpoint closes its receiver.
    pub fn closed(&self) -> Closed {
        Closed::new(&self.hangup_notify)
    }

    /// Convert this into a sink.
    pub fn into_sink(self) -> RawSenderSink {
        RawSenderSink::new(self)
    }
}

impl Drop for RawSender {
    fn drop(&mut self) {
        // required for correct drop order
    }
}

/// A sink sending byte data over a channel.
pub struct RawSenderSink {
    sender: Option<Arc<Mutex<RawSender>>>,
    send_fut: Option<BoxFuture<'static, Result<(), SendError>>>,
}

impl RawSenderSink {
    fn new(sender: RawSender) -> Self {
        Self { sender: Some(Arc::new(Mutex::new(sender))), send_fut: None }
    }

    async fn send(sender: Arc<Mutex<RawSender>>, data: Bytes) -> Result<(), SendError> {
        let mut sender = sender.lock().await;
        sender.send(data).await
    }

    fn start_send(&mut self, data: Bytes) -> Result<(), SendError> {
        if self.send_fut.is_some() {
            panic!("sink is not ready for sending");
        }

        match self.sender.clone() {
            Some(sender) => {
                self.send_fut = Some(Self::send(sender, data).boxed());
                Ok(())
            }
            None => Err(SendError::SinkClosed),
        }
    }

    fn poll_send(&mut self, cx: &mut Context) -> Poll<Result<(), SendError>> {
        match &mut self.send_fut {
            Some(fut) => {
                let res = ready!(fut.as_mut().poll(cx));
                self.send_fut = None;
                Poll::Ready(res)
            }
            None => Poll::Ready(Ok(())),
        }
    }

    fn close(&mut self) {
        self.sender = None;
    }
}

impl Sink<Bytes> for RawSenderSink {
    type Error = SendError;

    fn poll_ready(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Result<(), Self::Error>> {
        Pin::into_inner(self).poll_send(cx)
    }

    fn start_send(self: Pin<&mut Self>, item: Bytes) -> Result<(), Self::Error> {
        Pin::into_inner(self).start_send(item)
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Result<(), Self::Error>> {
        Pin::into_inner(self).poll_send(cx)
    }

    fn poll_close(mut self: Pin<&mut Self>, cx: &mut Context) -> Poll<Result<(), Self::Error>> {
        ready!(Pin::into_inner(self.as_mut()).poll_send(cx))?;
        Pin::into_inner(self).close();
        Poll::Ready(Ok(()))
    }
}

/// Sends items in serialized form over a channel.
#[derive(Debug)]
pub struct Sender<Item> {
    raw: RawSender,
    serializer: Box<dyn Serializer<Item>>,
}

impl<Item> Sender<Item>
where
    Item: Serialize,
{
    /// Create a sender that serializes items and sends them over a channel.
    pub fn new(raw: RawSender, serializer: Box<dyn Serializer<Item>>) -> Self {
        Self { raw, serializer }
    }

    /// Sends an items over the channel.
    ///
    /// Waits until send space becomes available.
    pub async fn send(&mut self, item: &Item) -> Result<(), SendError> {
        let data = self.serializer.serialize(item).map_err(SendError::SerializationError)?;
        self.raw.send(data).await
    }

    /// Tries to send an item over the channel.
    ///
    /// Does not wait until send space becomes available.
    pub fn try_send(&mut self, item: &Item) -> Result<(), TrySendError> {
        let data = self.serializer.serialize(item).map_err(|err| SendError::SerializationError(err))?;
        self.raw.try_send(&data)
    }

    /// True, once the remote endpoint has closed its receiver.
    pub fn is_closed(&self) -> bool {
        self.raw.is_closed()
    }

    /// Returns a Future that will resolve when the remote endpoint has closed its receiver.
    ///
    /// It will resolve immediately when the remote endpoint has already hung up.    
    pub fn closed(&self) -> Closed {
        self.raw.closed()
    }

    /// Convert this into a sink.
    pub fn into_sink(self) -> SenderSink<Item> {
        SenderSink::new(self)
    }

    /// Convert this into a raw sender.
    pub fn into_raw(self) -> RawSender {
        self.raw
    }
}

/// A sink sending items over a channel.
pub struct SenderSink<Item>
where
    Item: Serialize + 'static,
{
    sender: Option<Arc<Mutex<Sender<Item>>>>,
    send_fut: Option<LocalBoxFuture<'static, Result<(), SendError>>>,
}

impl<Item> SenderSink<Item>
where
    Item: Serialize + 'static,
{
    fn new(sender: Sender<Item>) -> Self {
        Self { sender: Some(Arc::new(Mutex::new(sender))), send_fut: None }
    }

    async fn send(sender: Arc<Mutex<Sender<Item>>>, item: Item) -> Result<(), SendError> {
        let mut sender = sender.lock().await;
        sender.send(&item).await
    }

    fn start_send(&mut self, item: Item) -> Result<(), SendError> {
        if self.send_fut.is_some() {
            panic!("sink is not ready for sending");
        }

        match self.sender.clone() {
            Some(sender) => {
                self.send_fut = Some(Self::send(sender, item).boxed_local());
                Ok(())
            }
            None => Err(SendError::SinkClosed),
        }
    }

    fn poll_send(&mut self, cx: &mut Context) -> Poll<Result<(), SendError>> {
        match &mut self.send_fut {
            Some(fut) => {
                let res = ready!(fut.as_mut().poll(cx));
                self.send_fut = None;
                Poll::Ready(res)
            }
            None => Poll::Ready(Ok(())),
        }
    }

    fn close(&mut self) {
        self.sender = None;
    }
}

impl<Item> Sink<Item> for SenderSink<Item>
where
    Item: Serialize,
{
    type Error = SendError;

    fn poll_ready(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Result<(), Self::Error>> {
        Pin::into_inner(self).poll_send(cx)
    }

    fn start_send(self: Pin<&mut Self>, item: Item) -> Result<(), Self::Error> {
        Pin::into_inner(self).start_send(item)
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Result<(), Self::Error>> {
        Pin::into_inner(self).poll_send(cx)
    }

    fn poll_close(mut self: Pin<&mut Self>, cx: &mut Context) -> Poll<Result<(), Self::Error>> {
        ready!(Pin::into_inner(self.as_mut()).poll_send(cx))?;
        Pin::into_inner(self).close();
        Poll::Ready(Ok(()))
    }
}
