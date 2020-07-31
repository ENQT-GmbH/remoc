use futures::{
    channel::{mpsc, oneshot},
    sink::SinkExt,
    Future,
};
use serde::{de::DeserializeOwned, Serialize};
use std::{clone::Clone, error::Error, fmt, marker::PhantomData};

use crate::{
    codec::ContentCodecFactory,
    receiver::{RawReceiver, Receiver},
    sender::{RawSender, Sender},
};

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
    codec_factory: Codec,
    _service_ghost: PhantomData<Service>,
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
    Codec: ContentCodecFactory<Content>,
{
    pub(crate) fn new(
        connect_tx: mpsc::Sender<ConnectToRemoteServiceRequest<Content>>, codec_factory: &Codec,
    ) -> Client<Service, Content, Codec> {
        Client { connect_tx, codec_factory: codec_factory.clone(), _service_ghost: PhantomData }
    }

    /// Connects to the specified service of the remote endpoint.
    /// If connection is accepted, a pair of channel sender and receiver is returned.
    ///
    /// Multiple connect requests can be outstanding at the same time.
    pub fn connect<SinkItem, StreamItem>(
        &self, service: Service,
    ) -> impl Future<Output = Result<(Sender<SinkItem>, Receiver<StreamItem>), ConnectError>>
    where
        SinkItem: 'static + Serialize,
        StreamItem: 'static + DeserializeOwned,
    {
        let mut connect_tx = self.connect_tx.clone();
        let codec_factory = self.codec_factory.clone();

        async move {
            let serializer = codec_factory.serializer();
            let service = serializer.serialize(service).map_err(ConnectError::SerializationError)?;

            let (response_tx, response_rx) = oneshot::channel();
            connect_tx
                .send(ConnectToRemoteServiceRequest { service, response_tx })
                .await
                .map_err(|_| ConnectError::MultiplexerError)?;
            match response_rx.await {
                Ok(ConnectToRemoteServiceResponse::Accepted(raw_sender, raw_receiver)) => {
                    let serializer = codec_factory.serializer();
                    let sender = Sender::new(raw_sender, serializer);
                    let deserializer = codec_factory.deserializer();
                    let receiver = Receiver::new(raw_receiver, deserializer);
                    Ok((sender, receiver))
                }
                Ok(ConnectToRemoteServiceResponse::Rejected) => Err(ConnectError::Rejected),
                Err(_) => Err(ConnectError::MultiplexerError),
            }
        }
    }
}

impl<Service, Content, Codec> Clone for Client<Service, Content, Codec>
where
    Service: Serialize + 'static,
    Content: Send + 'static,
    Codec: ContentCodecFactory<Content>,
{
    fn clone(&self) -> Self {
        Self::new(self.connect_tx.clone(), &self.codec_factory)
    }
}
