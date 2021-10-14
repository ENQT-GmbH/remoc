//! Initial connection functions.

use bytes::Bytes;
use futures::{Sink, Stream, TryStreamExt};
use std::{convert::TryInto, error::Error, fmt, io};
use tokio::io::{AsyncRead, AsyncWrite};
use tokio_util::codec::LengthDelimitedCodec;

use crate::{
    chmux::{self, ChMux, ChMuxError},
    codec::{self},
    rch::remote,
    RemoteSend,
};

/// Connection error.
#[derive(Debug, Clone)]
pub enum ConnectError<TransportSinkError, TransportStreamError> {
    /// Establishing chmux connection failed.
    ChMux(ChMuxError<TransportSinkError, TransportStreamError>),
    /// Opening initial remote channel failed.
    RemoteConnect(remote::ConnectError),
}

impl<TransportSinkError, TransportStreamError> fmt::Display
    for ConnectError<TransportSinkError, TransportStreamError>
where
    TransportSinkError: fmt::Display,
    TransportStreamError: fmt::Display,
{
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Self::ChMux(err) => write!(f, "chmux error: {}", err),
            Self::RemoteConnect(err) => write!(f, "channel connect failed: {}", err),
        }
    }
}

impl<TransportSinkError, TransportStreamError> Error for ConnectError<TransportSinkError, TransportStreamError>
where
    TransportSinkError: Error,
    TransportStreamError: Error,
{
}

impl<TransportSinkError, TransportStreamError> From<ChMuxError<TransportSinkError, TransportStreamError>>
    for ConnectError<TransportSinkError, TransportStreamError>
{
    fn from(err: ChMuxError<TransportSinkError, TransportStreamError>) -> Self {
        Self::ChMux(err)
    }
}

impl<TransportSinkError, TransportStreamError> From<remote::ConnectError>
    for ConnectError<TransportSinkError, TransportStreamError>
{
    fn from(err: remote::ConnectError) -> Self {
        Self::RemoteConnect(err)
    }
}

/// Establishes a connection over a framed transport and returns a remote sender and receiver.
///
/// This establishes a chmux connection over the transport and opens a remote channel.
///
/// The multiplexer is spawned into a separate task.
///
/// # Panics
/// Panics if the chmux configuration is invalid.
pub async fn connect_framed<TransportSink, TransportSinkError, TransportStream, TransportStreamError, T, Codec>(
    chmux_cfg: chmux::Cfg, transport_sink: TransportSink, transport_stream: TransportStream,
) -> Result<
    (remote::Sender<T, Codec>, remote::Receiver<T, Codec>),
    ConnectError<TransportSinkError, TransportStreamError>,
>
where
    TransportSink: Sink<Bytes, Error = TransportSinkError> + Send + Sync + Unpin + 'static,
    TransportSinkError: Error + Send + Sync + 'static,
    TransportStream: Stream<Item = Result<Bytes, TransportStreamError>> + Send + Sync + Unpin + 'static,
    TransportStreamError: Error + Send + Sync + 'static,
    T: RemoteSend,
    Codec: codec::Codec,
{
    let (mux, client, mut listener) = ChMux::new(chmux_cfg, transport_sink, transport_stream).await?;
    tokio::spawn(mux.run());
    Ok(remote::connect(&client, &mut listener).await?)
}

/// Establishes a connection over an IO transport and returns a remote sender and receiver.
///
/// This prepends a length header to each chmux packet for transportation over the unframed connection.
/// A chmux connection is established over the transport and a remote channel is opened.
///
/// The multiplexer is spawned into a separate task.
///
/// # Panics
/// Panics if the chmux configuration is invalid.
pub async fn connect_io<Read, Write, T, Codec>(
    chmux_cfg: chmux::Cfg, input: Read, output: Write,
) -> Result<(remote::Sender<T, Codec>, remote::Receiver<T, Codec>), ConnectError<io::Error, io::Error>>
where
    Read: AsyncRead + Send + Sync + Unpin + 'static,
    Write: AsyncWrite + Send + Sync + Unpin + 'static,
    T: RemoteSend,
    Codec: codec::Codec,
{
    let max_recv_frame_length: usize = chmux_cfg.max_frame_length().try_into().unwrap();
    let transport_sink = LengthDelimitedCodec::builder()
        .little_endian()
        .length_field_length(4)
        .max_frame_length(u32::MAX as _)
        .new_write(output);
    let transport_stream = LengthDelimitedCodec::builder()
        .little_endian()
        .length_field_length(4)
        .max_frame_length(max_recv_frame_length)
        .new_read(input)
        .map_ok(|item| item.freeze());
    connect_framed(chmux_cfg, transport_sink, transport_stream).await
}
