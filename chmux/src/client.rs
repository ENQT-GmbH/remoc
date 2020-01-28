use std::fmt;
use futures::channel::{oneshot, mpsc};
use futures::sink::{SinkExt};

use crate::raw_channel::RawChannel;

#[derive(Debug)]
pub enum MultiplexerConnectError {
    /// Connection has been rejected by server.
    Rejected,
    /// A multiplexer error has occured or it has been terminated.
    MultiplexerError
}


/// Connection to remote service request to local multiplexer.
pub struct ConnectToRemoteServiceRequest<Content> where Content: Send {
    /// Service to connect to.
    pub service: Content,
    /// Response channel sender.
    pub response_tx: oneshot::Sender<ConnectToRemoteServiceResponse<Content>>,
}

/// Connection to remote service response from local multiplexer.
pub enum ConnectToRemoteServiceResponse<Content> where Content: Send {
    /// Connection accepted and channel opened.
    Accepted (RawChannel<Content>),
    /// Connection rejected.
    Rejected 
}


/// Multiplexer client.
/// 
/// Allows to connect to remote services.
pub struct MultiplexerClient<Content> where Content: Send {
    pub(crate) connect_tx: mpsc::Sender<ConnectToRemoteServiceRequest<Content>>,
}

impl<Content> fmt::Debug for MultiplexerClient<Content> where Content: Send {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "MultiplexerClient")
    }
}

impl<Content> MultiplexerClient<Content> where Content: Send {
    /// Connects to the specified service of the remote endpoint.
    /// If connection is accepted, a pair of channel sender and receiver is returned.
    pub async fn connect(&mut self, service: Content) -> 
        Result<RawChannel<Content>, MultiplexerConnectError> 
    {
        let (response_tx, response_rx) = oneshot::channel();
        self.connect_tx.send(ConnectToRemoteServiceRequest {service, response_tx}).await.map_err(|_| MultiplexerConnectError::MultiplexerError)?;
        match response_rx.await {
            Ok(ConnectToRemoteServiceResponse::Accepted (raw_channel)) => Ok(raw_channel), 
            Ok(ConnectToRemoteServiceResponse::Rejected) => Err(MultiplexerConnectError::Rejected),
            Err(_) => Err(MultiplexerConnectError::MultiplexerError)
        }   
    }
}


// For the request it makes sense to have a common format, because that is what the server must match on.
// However, for reject it is a bit difficult, because it allows passing of one message.
// So we could make reason an optional adapted field.
// But this would mean that client and server both need additional serializers for little gain.
// Okay, so remove this whole reason thing.

// But how to adapt the client and server?
// Use the async trait crate.
// Let's see about the ergonomics later. Perhaps we can auto create the serializer/deserializer.

