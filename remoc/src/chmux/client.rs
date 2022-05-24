use futures::{ready, Future, FutureExt};
use std::{
    clone::Clone,
    error::Error,
    fmt, mem,
    pin::Pin,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    task::{Context, Poll},
};
use tokio::{
    sync::{mpsc, oneshot},
    task::JoinHandle,
};

use super::{
    port_allocator::{PortAllocator, PortNumber},
    receiver::Receiver,
    sender::Sender,
};

/// An error occurred during connecting to a remote service.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum ConnectError {
    /// All local ports are in use.
    LocalPortsExhausted,
    /// All remote ports are in use.
    RemotePortsExhausted,
    /// Too many connection requests are pending.
    TooManyPendingConnectionRequests,
    /// Connection has been rejected by server.
    Rejected,
    /// A multiplexer error has occurred or it has been terminated.
    ChMux,
}

impl fmt::Display for ConnectError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Self::LocalPortsExhausted => write!(f, "all local ports are in use"),
            Self::RemotePortsExhausted => write!(f, "all remote ports are in use"),
            Self::TooManyPendingConnectionRequests => write!(f, "too many connection requests are pending"),
            Self::Rejected => write!(f, "connection has been rejected by server"),
            Self::ChMux => write!(f, "multiplexer error"),
        }
    }
}

impl Error for ConnectError {}

impl From<ConnectError> for std::io::Error {
    fn from(err: ConnectError) -> Self {
        use std::io::ErrorKind;
        match err {
            ConnectError::LocalPortsExhausted => Self::new(ErrorKind::AddrInUse, err.to_string()),
            ConnectError::RemotePortsExhausted => Self::new(ErrorKind::AddrInUse, err.to_string()),
            ConnectError::TooManyPendingConnectionRequests => Self::new(ErrorKind::AddrInUse, err.to_string()),
            ConnectError::Rejected => Self::new(ErrorKind::ConnectionRefused, err.to_string()),
            ConnectError::ChMux => Self::new(ErrorKind::ConnectionReset, err.to_string()),
        }
    }
}

/// Accounts connection request credits.
#[derive(Clone)]
struct ConntectRequestCrediter(Arc<Mutex<ConntectRequestCrediterInner>>);

struct ConntectRequestCrediterInner {
    limit: u16,
    used: u16,
    notify_tx: Vec<oneshot::Sender<()>>,
}

impl ConntectRequestCrediter {
    /// Creates a new connection request crediter.
    pub fn new(limit: u16) -> Self {
        let inner = ConntectRequestCrediterInner { limit, used: 0, notify_tx: Vec::new() };
        Self(Arc::new(Mutex::new(inner)))
    }

    /// Obtains a connection request credit.
    ///
    /// Waits for the credit to become available.
    pub async fn request(&self) -> ConnectRequestCredit {
        loop {
            let rx = {
                let mut inner = self.0.lock().unwrap();

                if inner.used < inner.limit {
                    inner.used += 1;
                    return ConnectRequestCredit(self.0.clone());
                } else {
                    let (tx, rx) = oneshot::channel();
                    inner.notify_tx.push(tx);
                    rx
                }
            };

            let _ = rx.await;
        }
    }

    /// Tries to obtain a connection request credit.
    ///
    /// Does not wait for the credit to become available.
    pub fn try_request(&self) -> Option<ConnectRequestCredit> {
        let mut inner = self.0.lock().unwrap();

        if inner.used < inner.limit {
            inner.used += 1;
            Some(ConnectRequestCredit(self.0.clone()))
        } else {
            None
        }
    }
}

/// A credit for requesting a connection.
pub(crate) struct ConnectRequestCredit(Arc<Mutex<ConntectRequestCrediterInner>>);

impl Drop for ConnectRequestCredit {
    fn drop(&mut self) {
        let notify_tx = {
            let mut inner = self.0.lock().unwrap();
            inner.used -= 1;
            mem::take(&mut inner.notify_tx)
        };

        for tx in notify_tx {
            let _ = tx.send(());
        }
    }
}

/// Connection to remote service request to local multiplexer.
#[derive(Debug)]
pub(crate) struct ConnectRequest {
    /// Local port.
    pub local_port: PortNumber,
    /// Notification that request has been queued for sending.
    pub sent_tx: mpsc::Sender<()>,
    /// Response channel sender.
    pub response_tx: oneshot::Sender<ConnectResponse>,
    /// Wait for port to become available.
    pub wait: bool,
}

/// Connection to remote service response from local multiplexer.
#[derive(Debug)]
#[allow(clippy::large_enum_variant)]
pub(crate) enum ConnectResponse {
    /// Connection accepted and channel opened.
    Accepted(Sender, Receiver),
    /// Connection was rejected.
    Rejected {
        /// Remote endpoint had not ports available.
        no_ports: bool,
    },
}

/// An outstanding connection request.
///
/// Await it to obtain the result of the connection request.
pub struct Connect {
    pub(crate) sent_rx: mpsc::Receiver<()>,
    pub(crate) response: JoinHandle<Result<(Sender, Receiver), ConnectError>>,
}

impl Connect {
    /// Returns once the connect request has been sent.
    ///
    /// It is guaranteed that the connect request will be made available via
    /// the [Listener](super::Listener) at the remote endpoint before messages
    /// sent on any port after this function returns will arrive.
    ///
    /// This will also return when the multiplexer has been terminated.
    pub async fn sent(&mut self) {
        let _ = self.sent_rx.recv().await;
    }
}

impl Future for Connect {
    type Output = Result<(Sender, Receiver), ConnectError>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
        let result = ready!(Pin::into_inner(self).response.poll_unpin(cx));
        Poll::Ready(result.map_err(|_| ConnectError::ChMux)?)
    }
}

/// Multiplexer client.
///
/// Use to request a new port for sending and receiving.
/// This can be cloned to make simultaneous requests.
#[derive(Clone)]
pub struct Client {
    tx: mpsc::UnboundedSender<ConnectRequest>,
    crediter: ConntectRequestCrediter,
    port_allocator: PortAllocator,
    listener_dropped: Arc<AtomicBool>,
    terminate_tx: mpsc::UnboundedSender<()>,
}

impl fmt::Debug for Client {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("Client").field("port_allocator", &self.port_allocator).finish()
    }
}

impl Client {
    pub(crate) fn new(
        tx: mpsc::UnboundedSender<ConnectRequest>, limit: u16, port_allocator: PortAllocator,
        listener_dropped: Arc<AtomicBool>, terminate_tx: mpsc::UnboundedSender<()>,
    ) -> Client {
        Client {
            tx,
            crediter: ConntectRequestCrediter::new(limit),
            port_allocator,
            listener_dropped,
            terminate_tx,
        }
    }

    /// Obtains the port allocator.
    pub fn port_allocator(&self) -> PortAllocator {
        self.port_allocator.clone()
    }

    /// Connects to a newly allocated remote port from a newly allocated local port.
    ///
    /// This function waits until a local and remote port become available.
    pub async fn connect(&self) -> Result<(Sender, Receiver), ConnectError> {
        self.connect_ext(None, true).await?.await
    }

    /// Start opening a new port to the remote endpoint with extended options.
    ///
    /// If `local_port` is [None] a new local port number is allocated.
    /// Otherwise the specified port is used.
    ///
    /// If `wait` is true, this function waits until a local and remote port become available.
    /// Otherwise it returns the appropriate [ConnectError] if no ports are available.
    /// If `wait` is false, it still waits until the listener on the remote endpoint accepts
    /// or rejects the connection.
    ///
    /// This returns a [Connect] that must be awaited to obtain the result.
    pub async fn connect_ext(&self, local_port: Option<PortNumber>, wait: bool) -> Result<Connect, ConnectError> {
        // Obtain local port.
        let local_port = match local_port {
            Some(local_port) => local_port,
            None => {
                if wait {
                    self.port_allocator.allocate().await
                } else {
                    self.port_allocator.try_allocate().ok_or(ConnectError::LocalPortsExhausted)?
                }
            }
        };

        // Obtain credit for connection request.
        let credit = if wait {
            self.crediter.request().await
        } else {
            match self.crediter.try_request() {
                Some(credit) => credit,
                None => return Err(ConnectError::TooManyPendingConnectionRequests),
            }
        };

        // Build and send request.
        let (sent_tx, sent_rx) = mpsc::channel(1);
        let (response_tx, response_rx) = oneshot::channel();
        let req = ConnectRequest { local_port, sent_tx, response_tx, wait };
        let _ = self.tx.send(req);

        let listener_dropped = self.listener_dropped.clone();
        let response = tokio::spawn(async move {
            // Credit must be kept until response is received.
            let _credit = credit;

            // Process response.
            match response_rx.await {
                Ok(ConnectResponse::Accepted(sender, receiver)) => Ok((sender, receiver)),
                Ok(ConnectResponse::Rejected { no_ports }) => {
                    if no_ports {
                        Err(ConnectError::RemotePortsExhausted)
                    } else {
                        Err(ConnectError::Rejected)
                    }
                }
                Err(_) => {
                    if listener_dropped.load(Ordering::SeqCst) {
                        Err(ConnectError::Rejected)
                    } else {
                        Err(ConnectError::ChMux)
                    }
                }
            }
        });

        Ok(Connect { sent_rx, response })
    }

    /// Terminates the multiplexer, forcibly closing all open ports.
    pub fn terminate(&self) {
        let _ = self.terminate_tx.send(());
    }
}
