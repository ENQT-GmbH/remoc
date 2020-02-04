use futures::channel::{mpsc, oneshot};
use futures::sink::SinkExt;
use serde::de::DeserializeOwned;
use serde::Serialize;
use std::clone::Clone;
use std::error::Error;
use std::fmt;

use crate::codec::{CodecFactory, Serializer};
use crate::receiver::{RawReceiver, Receiver};
use crate::sender::{RawSender, Sender};

/// An error occured during connecting to a remote service.
#[derive(Debug)]
pub enum ConnectError {
    /// Connection has been rejected by server.
    Rejected,
    /// A multiplexer error has occured or it has been terminated.
    MultiplexerError,
    /// Error serializing the service request.
    SerializationError(Box<dyn Error + Send + 'static>),
}

impl fmt::Display for ConnectError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Self::Rejected => write!(f, "Connection has been rejected by server."),
            Self::MultiplexerError => write!(f, "A multiplexer error has occured or it has been terminated."),
            Self::SerializationError(err) => write!(f, "A serialization error occured: {}", err),
        }
    }
}

impl Error for ConnectError {}

/// Connection to remote service request to local multiplexer.
pub struct ConnectToRemoteServiceRequest<Content>
where
    Content: Send,
{
    /// Service to connect to.
    pub service: Content,
    /// Response channel sender.
    pub response_tx: oneshot::Sender<ConnectToRemoteServiceResponse<Content>>,
}

/// Connection to remote service response from local multiplexer.
pub enum ConnectToRemoteServiceResponse<Content>
where
    Content: Send,
{
    /// Connection accepted and channel opened.
    Accepted(RawSender<Content>, RawReceiver<Content>),
    /// Connection rejected.
    Rejected,
}

/// Raw multiplexer client.
///
/// Allows to connect to remote services.
///
/// A client can be cloned to allow multiple simultaneous service requests.
pub struct Client<Service, Content, Codec>
where
    Content: Send,
{
    pub(crate) connect_tx: mpsc::Sender<ConnectToRemoteServiceRequest<Content>>,
    serializer: Box<dyn Serializer<Service, Content>>,
    codec_factory: Codec,
}

impl<Service, Content, Codec> fmt::Debug for Client<Service, Content, Codec>
where
    Content: Send,
{
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "Client")
    }
}

impl<Service, Content, Codec> Client<Service, Content, Codec>
where
    Service: Serialize + 'static,
    Content: Send + 'static,
    Codec: CodecFactory<Content>,
{
    pub(crate) fn new(
        connect_tx: mpsc::Sender<ConnectToRemoteServiceRequest<Content>>, codec_factory: &Codec,
    ) -> Client<Service, Content, Codec> {
        Client { connect_tx, serializer: codec_factory.serializer(), codec_factory: codec_factory.clone() }
    }

    /// Connects to the specified service of the remote endpoint.
    /// If connection is accepted, a pair of channel sender and receiver is returned.
    pub async fn connect<SinkItem, StreamItem>(
        &mut self, service: Service,
    ) -> Result<(Sender<SinkItem>, Receiver<StreamItem>), ConnectError>
    where
        SinkItem: 'static + Serialize,
        StreamItem: 'static + DeserializeOwned,
    {
        let (response_tx, response_rx) = oneshot::channel();
        let service = self.serializer.serialize(service).map_err(ConnectError::SerializationError)?;
        self.connect_tx
            .send(ConnectToRemoteServiceRequest { service, response_tx })
            .await
            .map_err(|_| ConnectError::MultiplexerError)?;
        match response_rx.await {
            Ok(ConnectToRemoteServiceResponse::Accepted(raw_sender, raw_receiver)) => {
                let serializer = self.codec_factory.serializer();
                let sender = Sender::new(raw_sender, serializer);
                let deserializer = self.codec_factory.deserializer();
                let receiver = Receiver::new(raw_receiver, deserializer);
                Ok((sender, receiver))
            }
            Ok(ConnectToRemoteServiceResponse::Rejected) => Err(ConnectError::Rejected),
            Err(_) => Err(ConnectError::MultiplexerError),
        }
    }
}

impl<Service, Content, Codec> Clone for Client<Service, Content, Codec>
where
    Service: Serialize + 'static,
    Content: Send + 'static,
    Codec: CodecFactory<Content>,
{
    fn clone(&self) -> Self {
        Self::new(self.connect_tx.clone(), &self.codec_factory)
    }
}
