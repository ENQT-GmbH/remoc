//! A channel that exchanges arbitrary vales with a remote endpoint.

use serde::{Deserialize, Serialize};
use std::{error::Error, fmt};

mod io;
mod receiver;
mod sender;

pub use receiver::{PortDeserializer, Receiver, RecvError};
pub use sender::{Closed, PortSerializer, SendError, SendErrorKind, Sender};

use crate::{chmux, codec, RemoteSend};

/// Chunk queue length for big data (de-)serialization.
const BIG_DATA_CHUNK_QUEUE: usize = 32;

/// Limit for counting big data instances.
const BIG_DATA_LIMIT: i8 = 16;

/// Creating the remote channel failed.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ConnectError {
    /// The connect request failed.
    Connect(chmux::ConnectError),
    /// Listening for the remote connect request failed.
    Listen(chmux::ListenerError),
    /// The remote endpoint did not send a connect request.
    NoConnectRequest,
}

impl fmt::Display for ConnectError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            ConnectError::Connect(err) => write!(f, "connect error: {}", err),
            ConnectError::Listen(err) => write!(f, "listen error: {}", err),
            ConnectError::NoConnectRequest => write!(f, "no connect request received"),
        }
    }
}

impl Error for ConnectError {}

impl From<chmux::ConnectError> for ConnectError {
    fn from(err: chmux::ConnectError) -> Self {
        Self::Connect(err)
    }
}

impl From<chmux::ListenerError> for ConnectError {
    fn from(err: chmux::ListenerError) -> Self {
        Self::Listen(err)
    }
}

/// Create a remote channel over an existing chmux connection.
pub async fn connect<T, Codec>(
    client: &chmux::Client, listener: &mut chmux::Listener,
) -> Result<(Sender<T, Codec>, Receiver<T, Codec>), ConnectError>
where
    T: RemoteSend,
    Codec: codec::Codec,
{
    let (client_sr, listener_sr) = tokio::join!(client.connect(), listener.accept());
    let (raw_sender, _) = client_sr?;
    let (_, raw_receiver) = listener_sr?.ok_or(ConnectError::NoConnectRequest)?;
    Ok((Sender::new(raw_sender), Receiver::new(raw_receiver)))
}
