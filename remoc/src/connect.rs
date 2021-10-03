//! Initial connection functions.

use bytes::Bytes;
use futures::{Sink, Stream};
use std::{error::Error, fmt};
use serde::{Serialize, Deserialize};

use crate::{
    chmux::{self, ChMux, ChMuxError},
    codec::CodecT,
    rsync::{remote, RemoteSend},
};

pub enum ConnectError<TransportSinkError, TransportStreamError> {
    ChMux(ChMuxError<TransportSinkError, TransportStreamError>),
    Connect(chmux::ConnectError),
    Listen(chmux::ListenerError),
}

pub async fn connect<TransportSink, TransportSinkError, TransportStream, TransportStreamError, T, Codec>(
    chmux_cfg: &chmux::Cfg, transport_sink: TransportSink, transport_stream: TransportStream,
) -> Result<
    (ChMux<TransportSink, TransportStream>, remote::Sender<T, Codec>, remote::Receiver<T, Codec>),
    ChMuxError<TransportSinkError, TransportStreamError>,
>
where
    TransportSink: Sink<Bytes, Error = TransportSinkError> + Send + Sync + Unpin + 'static,
    TransportSinkError: Error + Send + Sync + 'static,
    TransportStream: Stream<Item = Result<Bytes, TransportStreamError>> + Send + Sync + Unpin + 'static,
    TransportStreamError: Error + Send + Sync + 'static,
    T: RemoteSend,
    Codec: CodecT,
{
    let (mux, client, listener) = ChMux::new(chmux_cfg, transport_sink, transport_stream).await?;
    tokio::spawn(mux.run());
    connect_client_listener(&client, &mut listener).await
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ChannelConnectError {
    Connect(chmux::ConnectError),
    Listen(chmux::ListenerError),
    NoConnectRequest,
}

impl fmt::Display for ChannelConnectError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            ChannelConnectError::Connect(err) => write!(f, "connect error: {}", err),
            ChannelConnectError::Listen(err) => write!(f, "listen error: {}", err),
            ChannelConnectError::NoConnectRequest => write!(f, "no connect request received"),
        }
    }
}

impl Error for ChannelConnectError {}

impl From<chmux::ConnectError> for ChannelConnectError {
    fn from(err: chmux::ConnectError) -> Self {
        Self::Connect(err)
    }
}

impl From<chmux::ListenerError> for ChannelConnectError {
    fn from(err: chmux::ListenerError) -> Self {
        Self::NoConnectRequest
    }
}

pub async fn connect_client_listener<T, Codec>(
    client: &chmux::Client, listener: &mut chmux::Listener,
) -> Result<(remote::Sender<T, Codec>, remote::Receiver<T, Codec>), ChannelConnectError>
where
    T: RemoteSend,
    Codec: CodecT,
{
    let (client_sr, listener_sr) = tokio::join!(client.connect(), listener.accept());
    let (raw_sender, _) = client_sr?;
    let (_, raw_receiver) = listener_sr?.ok_or(ChannelConnectError::NoConnectRequest)?;
    Ok((remote::Sender::new(raw_sender), remote::Receiver::new(raw_receiver)))
}
