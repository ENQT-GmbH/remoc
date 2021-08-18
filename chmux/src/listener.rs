use futures::{
    future::BoxFuture,
    ready,
    stream::Stream,
    task::{Context, Poll},
    FutureExt,
};
use std::{error::Error, fmt, pin::Pin, sync::Arc};
use tokio::sync::{mpsc, oneshot, Mutex};

use crate::{
    multiplexer::PortEvt,
    port_allocator::{PortAllocator, PortNumber},
    receiver::RawReceiver,
    sender::RawSender,
};

/// An multiplexer listener error.
#[derive(Debug, Clone)]
pub enum ListenerError {
    /// All local ports are in use.
    LocalPortsExhausted,
    /// A multiplexer error has occured or it has been terminated.
    MultiplexerError,
}

impl fmt::Display for ListenerError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Self::LocalPortsExhausted => write!(f, "all local ports are in use"),
            Self::MultiplexerError => write!(f, "multiplexer error"),
        }
    }
}

impl Error for ListenerError {}

/// A connection request by the remote endpoint.
///
/// Dropping the request rejects it.
pub struct Request {
    remote_port: u32,
    wait: bool,
    allocator: PortAllocator,
    tx: mpsc::Sender<PortEvt>,
    done_tx: Option<oneshot::Sender<()>>,
}

impl Request {
    pub(crate) fn new(remote_port: u32, wait: bool, allocator: PortAllocator, tx: mpsc::Sender<PortEvt>) -> Self {
        let (done_tx, done_rx) = oneshot::channel();
        let drop_tx = tx.clone();
        tokio::spawn(async move {
            if done_rx.await.is_err() {
                let _ = drop_tx.send(PortEvt::Rejected { remote_port, no_ports: false }).await;
            }
        });

        Self { remote_port, wait, allocator, tx, done_tx: Some(done_tx) }
    }

    /// The remote port number.
    pub fn remote_port(&self) -> u32 {
        self.remote_port
    }

    /// Indicates whether the handler of the request should wait for a local
    /// port to become available, if all are currently in use.
    pub fn is_wait(&self) -> bool {
        self.wait
    }

    /// Accepts the request using a newly allocated local port.
    pub async fn accept(self) -> Result<(RawSender, RawReceiver), ListenerError> {
        let local_port = if self.wait {
            self.allocator.allocate().await
        } else {
            match self.allocator.try_allocate() {
                Some(local_port) => local_port,
                None => {
                    self.reject(true).await;
                    return Err(ListenerError::LocalPortsExhausted);
                }
            }
        };

        self.accept_from(local_port).await
    }

    /// Accepts the request using the specified local port.
    pub async fn accept_from(
        mut self, local_port: PortNumber,
    ) -> Result<(RawSender, RawReceiver), ListenerError> {
        let (port_tx, port_rx) = oneshot::channel();
        let _ = self.tx.send(PortEvt::Accepted { local_port, remote_port: self.remote_port, port_tx }).await;
        let _ = self.done_tx.take().unwrap().send(());

        port_rx.await.map_err(|_| ListenerError::MultiplexerError)
    }

    /// Rejects the connect request.
    ///
    /// Setting `no_ports` to true indicates to the remote endpoint that the request
    /// was rejected because no local port could be allocated.
    pub async fn reject(mut self, no_ports: bool) {
        let _ = self.tx.send(PortEvt::Rejected { remote_port: self.remote_port, no_ports }).await;
        let _ = self.done_tx.take().unwrap().send(());
    }
}

impl Drop for Request {
    fn drop(&mut self) {
        // required for correct drop order
    }
}

/// Remote connect message.
pub(crate) enum RemoteConnectMsg {
    /// Remote connect request.
    Request(Request),
    /// Client of remote endpoint has been dropped.
    ClientDropped,
}

/// Raw multiplexer listener.
pub struct RawListener {
    wait_rx: mpsc::Receiver<RemoteConnectMsg>,
    no_wait_rx: mpsc::Receiver<RemoteConnectMsg>,
    port_allocator: PortAllocator,
    closed: bool,
}

impl fmt::Debug for RawListener {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("RawListener").field("port_allocator", &self.port_allocator).finish()
    }
}

impl RawListener {
    pub(crate) fn new(
        wait_rx: mpsc::Receiver<RemoteConnectMsg>, no_wait_rx: mpsc::Receiver<RemoteConnectMsg>,
        port_allocator: PortAllocator,
    ) -> Self {
        Self { wait_rx, no_wait_rx, port_allocator, closed: false }
    }

    /// Obtains the port allocator.
    pub fn port_allocator(&self) -> PortAllocator {
        self.port_allocator.clone()
    }

    /// Accept a connection returning the raw sender and raw receiver for the opened port.
    ///
    /// Returns [None] when the client of the remote endpoint has been dropped and
    /// no more connection requests can be made.
    pub async fn accept(&mut self) -> Result<Option<(RawSender, RawReceiver)>, ListenerError> {
        if self.closed {
            return Ok(None);
        }

        loop {
            tokio::select! {
                local_port = self.port_allocator.allocate() => {
                    match self.inspect().await? {
                        Some(req) => break Ok(Some(req.accept_from(local_port).await?)),
                        None => break Ok(None),
                    }
                },

                no_wait_req_opt = self.no_wait_rx.recv() => {
                    match no_wait_req_opt {
                        Some(RemoteConnectMsg::Request(no_wait_req)) => {
                            match self.port_allocator.try_allocate() {
                                Some(local_port) => break Ok(Some(no_wait_req.accept_from(local_port).await?)),
                                None => no_wait_req.reject(true).await,
                            }
                        },
                        Some(RemoteConnectMsg::ClientDropped) => {
                            self.closed = true;
                            break Ok(None);
                        },
                        None => break Err(ListenerError::MultiplexerError),
                    }
                },
            }
        }
    }

    /// Obtains the next connection request from the remote endpoint.
    ///
    /// Connection requests can be stored and accepted or rejected at a later time.
    /// The maximum number of unanswered connection requests is specified in the
    /// configuration. If this number is reached, the remote endpoint will
    /// not send any more connection requests.
    ///
    /// Returns [None] when the client of the remote endpoint has been dropped and
    /// no more connection requests can be made.
    pub async fn inspect(&mut self) -> Result<Option<Request>, ListenerError> {
        if self.closed {
            return Ok(None);
        }

        let req_opt = tokio::select! {
            req_opt = self.wait_rx.recv() => req_opt,
            req_opt = self.no_wait_rx.recv() => req_opt,
        };

        match req_opt {
            Some(RemoteConnectMsg::Request(req)) => Ok(Some(req)),
            Some(RemoteConnectMsg::ClientDropped) => {
                self.closed = true;
                Ok(None)
            }
            None => Err(ListenerError::MultiplexerError),
        }
    }

    /// Convert this into a raw server stream.
    pub fn into_stream(self) -> RawListenerStream {
        RawListenerStream::new(self)
    }
}

impl Drop for RawListener {
    fn drop(&mut self) {
        // required for correct drop order
    }
}

/// A stream accepting connections and returning raw senders and raw receivers.
///
/// Ends when the client is dropped at the remote endpoint.
pub struct RawListenerStream {
    server: Arc<Mutex<RawListener>>,
    #[allow(clippy::type_complexity)]
    accept_fut: Option<BoxFuture<'static, Option<Result<(RawSender, RawReceiver), ListenerError>>>>,
}

impl RawListenerStream {
    fn new(server: RawListener) -> Self {
        Self { server: Arc::new(Mutex::new(server)), accept_fut: None }
    }

    async fn accept(server: Arc<Mutex<RawListener>>) -> Option<Result<(RawSender, RawReceiver), ListenerError>> {
        let mut server = server.lock().await;
        server.accept().await.transpose()
    }

    fn poll_next(&mut self, cx: &mut Context) -> Poll<Option<Result<(RawSender, RawReceiver), ListenerError>>> {
        if self.accept_fut.is_none() {
            self.accept_fut = Some(Self::accept(self.server.clone()).boxed());
        }

        let accept_fut = self.accept_fut.as_mut().unwrap();
        let res = ready!(accept_fut.as_mut().poll(cx));

        self.accept_fut = None;
        Poll::Ready(res)
    }
}

impl Stream for RawListenerStream {
    type Item = Result<(RawSender, RawReceiver), ListenerError>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Option<Self::Item>> {
        Pin::into_inner(self).poll_next(cx)
    }
}
