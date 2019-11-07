use std::fmt;
use futures::channel::{oneshot, mpsc};
use futures::sink::{SinkExt};

use crate::sender::ChannelSender;
use crate::receiver::ChannelReceiver;

#[derive(Debug)]
pub enum MultiplexerConnectError<Content> {
    /// Connection has been rejected by server with the optionally specified reason.
    Rejected (Option<Content>),
    /// A multiplexer error has occured or it has been terminated.
    MultiplexerError
}


pub struct ConnectToRemoteServiceRequest<Content> {
    pub service: Content,
    pub response_tx: oneshot::Sender<ConnectToRemoteServiceResponse<Content>>,
}

pub enum ConnectToRemoteServiceResponse<Content> {
    Accepted {
        sender: ChannelSender<Content>,
        receiver: ChannelReceiver<Content>,
    },
    Rejected {
        reason: Option<Content>
    }
}


pub struct MultiplexerClient<Content> {
    pub(crate) connect_tx: mpsc::Sender<ConnectToRemoteServiceRequest<Content>>,
}

impl<Content> fmt::Debug for MultiplexerClient<Content> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "MultiplexerClient")
    }
}


impl<Content> MultiplexerClient<Content> {
    /// Connects to the specified service of the remote endpoint.
    /// If connection is accepted, a pair of channel sender and receiver is returned.
    pub async fn connect(&mut self, service: Content) -> 
        Result<(ChannelSender<Content>, ChannelReceiver<Content>), MultiplexerConnectError<Content>> 
    {
        let (response_tx, response_rx) = oneshot::channel();
        self.connect_tx.send(ConnectToRemoteServiceRequest {service, response_tx}).await.map_err(|_| MultiplexerConnectError::MultiplexerError)?;
        match response_rx.await {
            Ok(ConnectToRemoteServiceResponse::Accepted {sender, receiver}) => Ok((sender, receiver)),
            Ok(ConnectToRemoteServiceResponse::Rejected {reason}) => Err(MultiplexerConnectError::Rejected (reason)),
            Err(_) => Err(MultiplexerConnectError::MultiplexerError)
        }   
    }
}