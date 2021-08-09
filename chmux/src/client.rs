use serde::{de::DeserializeOwned, Serialize};
use std::{
    clone::Clone,
    error::Error,
    fmt, mem,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
};
use tokio::sync::{mpsc, oneshot};

use crate::{
    codec::{BoxError, CodecFactory},
    port_allocator::{PortAllocator, PortNumber},
    receiver::{RawReceiver, Receiver},
    sender::{RawSender, Sender},
};

/// An error occured during connecting to a remote service.
#[derive(Debug)]
pub enum ConnectError {
    /// All local ports are in use.
    LocalPortsExhausted,
    /// All remote ports are in use.
    RemotePortsExhausted,
    /// Too many connection requests are pending.
    TooManyPendingConnectionRequests,
    /// Connection has been rejected by server.
    Rejected,
    /// A multiplexer error has occured or it has been terminated.
    MultiplexerError,
    /// Error serializing the service request.
    SerializationError(BoxError),
}

impl fmt::Display for ConnectError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Self::LocalPortsExhausted => write!(f, "all local ports are in use"),
            Self::RemotePortsExhausted => write!(f, "all remote ports are in use"),
            Self::TooManyPendingConnectionRequests => write!(f, "too many connection requests are pending"),
            Self::Rejected => write!(f, "connection has been rejected by server"),
            Self::MultiplexerError => write!(f, "multiplexer error"),
            Self::SerializationError(err) => write!(f, "serialization error: {}", err),
        }
    }
}

impl Error for ConnectError {}

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
    /// Response channel sender.
    pub response_tx: oneshot::Sender<ConnectResponse>,
    /// Wait for port to become available.
    pub wait: bool,
}

/// Connection to remote service response from local multiplexer.
#[derive(Debug)]
pub(crate) enum ConnectResponse {
    /// Connection accepted and channel opened.
    Accepted(RawSender, RawReceiver),
    /// Connection was rejected.
    Rejected {
        /// Remote endpoint had not ports available.
        no_ports: bool,
    },
}

/// Raw multiplexer client.
///
/// Use to request a new port for raw sending and raw receiving.
/// This can be cloned to make simultaneous requests.
#[derive(Clone)]
pub struct RawClient {
    tx: mpsc::UnboundedSender<ConnectRequest>,
    crediter: ConntectRequestCrediter,
    port_allocator: PortAllocator,
    listener_dropped: Arc<AtomicBool>,
}

impl fmt::Debug for RawClient {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("RawClient").field("port_allocator", &self.port_allocator).finish()
    }
}

impl RawClient {
    pub(crate) fn new(
        tx: mpsc::UnboundedSender<ConnectRequest>, limit: u16, port_allocator: PortAllocator,
        listener_dropped: Arc<AtomicBool>,
    ) -> RawClient {
        RawClient { tx, crediter: ConntectRequestCrediter::new(limit), port_allocator, listener_dropped }
    }

    /// Obtains the port allocator.
    pub fn port_allocator(&self) -> PortAllocator {
        self.port_allocator.clone()
    }

    /// Opens a new raw port.
    ///
    /// If `wait` is true, this function waits until a local and remote port become available.
    /// Otherwise it returns the appropriate error if no ports are available.
    async fn connect_int(
        &self, local_port: Option<PortNumber>, wait: bool,
    ) -> Result<(RawSender, RawReceiver), ConnectError> {
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
        let (response_tx, response_rx) = oneshot::channel();
        let req = ConnectRequest { local_port, response_tx, wait };
        let _ = self.tx.send(req);

        let listener_dropped = self.listener_dropped.clone();
        tokio::spawn(async move {
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
                        Err(ConnectError::MultiplexerError)
                    }
                }
            }
        })
        .await
        .map_err(|_| ConnectError::MultiplexerError)?
    }

    /// Connects to a newly allocated remote port from a newly allocated local port.
    ///
    /// This function waits until a local and remote port become available.
    pub async fn connect(&self) -> Result<(RawSender, RawReceiver), ConnectError> {
        self.connect_int(None, true).await
    }

    /// Connects to a newly allocated remote port from the specified local port.
    ///
    /// This function waits until a remote port becomes available.
    pub async fn connect_from(&self, local_port: PortNumber) -> Result<(RawSender, RawReceiver), ConnectError> {
        self.connect_int(Some(local_port), true).await
    }

    /// Tries to connect to a newly allocated remote port from a newly allocated local port.
    ///
    /// This function does not wait until a local and remote port becomes available.
    /// However, it still waits until the listener on the remote endpoint accepts or rejects the connection.
    pub async fn try_connect(&self) -> Result<(RawSender, RawReceiver), ConnectError> {
        self.connect_int(None, false).await
    }

    /// Tries to connect to a newly allocated remote port from the specified local port.
    ///
    /// This function does not wait until a remote port becomes available.
    /// However, it still waits until the listener on the remote endpoint accepts or rejects the connection.
    pub async fn try_connect_from(
        &self, local_port: PortNumber,
    ) -> Result<(RawSender, RawReceiver), ConnectError> {
        self.connect_int(Some(local_port), false).await
    }
}

/// Multiplexer client.
///
/// Use to request a new port for raw sending and raw receiving.
/// This can be cloned to make simultaneous requests.
#[derive(Clone, Debug)]
pub struct Client<Codec>
where
    Codec: CodecFactory,
{
    raw: RawClient,
    codec: Codec,
}

impl<Codec> Client<Codec>
where
    Codec: CodecFactory,
{
    /// Creates a new client.
    pub fn new(raw: RawClient, codec: Codec) -> Self {
        Self { raw, codec }
    }

    /// Convert this into a raw client.
    pub fn into_raw(self) -> RawClient {
        self.raw
    }

    /// Opens a new port.
    ///
    /// This function waits until a local and remote port become available.
    pub async fn connect<SendItem, ReceiveItem>(
        &self,
    ) -> Result<(Sender<SendItem>, Receiver<ReceiveItem>), ConnectError>
    where
        SendItem: Serialize + 'static,
        ReceiveItem: DeserializeOwned + 'static,
    {
        self.raw.connect().await.map(|(raw_sender, raw_receiver)| {
            (
                Sender::new(raw_sender, self.codec.serializer()),
                Receiver::new(raw_receiver, self.codec.deserializer()),
            )
        })
    }

    /// Opens a new port.
    ///
    /// This function does not wait until a local and remote port becomes available.
    /// However, it still waits until the listener on the remote endpoint accepts or rejects the connection.
    pub async fn try_connect<SendItem, ReceiveItem>(
        &self,
    ) -> Result<(Sender<SendItem>, Receiver<ReceiveItem>), ConnectError>
    where
        SendItem: Serialize + 'static,
        ReceiveItem: DeserializeOwned + 'static,
    {
        self.raw.try_connect().await.map(|(raw_sender, raw_receiver)| {
            (
                Sender::new(raw_sender, self.codec.serializer()),
                Receiver::new(raw_receiver, self.codec.deserializer()),
            )
        })
    }
}
