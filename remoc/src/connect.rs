//! Initial connection functions.

use bytes::Bytes;
use futures::{future::BoxFuture, Future, FutureExt, Sink, Stream, TryStreamExt};
use std::{
    convert::TryInto,
    error::Error,
    fmt, io,
    pin::Pin,
    task::{Context, Poll},
};
use tokio::io::{AsyncRead, AsyncWrite, BufReader, BufWriter};
use tokio_util::codec::LengthDelimitedCodec;

use crate::{
    chmux::{self, ChMux, ChMuxError},
    codec,
    rch::base,
    RemoteSend,
};

/// Error occured during establishing a connection over a physical transport.
#[cfg_attr(docsrs, doc(cfg(feature = "rch")))]
#[derive(Debug, Clone)]
pub enum ConnectError<TransportSinkError, TransportStreamError> {
    /// Establishing [chmux] connection failed.
    ChMux(ChMuxError<TransportSinkError, TransportStreamError>),
    /// Opening initial [remote](crate::rch::base) channel failed.
    RemoteConnect(base::ConnectError),
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

impl<TransportSinkError, TransportStreamError> From<base::ConnectError>
    for ConnectError<TransportSinkError, TransportStreamError>
{
    fn from(err: base::ConnectError) -> Self {
        Self::RemoteConnect(err)
    }
}

/// Methods for establishing a connection over a physical transport.
///
/// You must poll this returned future or spawn it onto a task for the connection to work.
#[cfg_attr(docsrs, doc(cfg(feature = "rch")))]
#[must_use = "You must poll or spawn the Connect future for the connection to work."]
pub struct Connect<'transport, TransportSinkError, TransportStreamError>(
    BoxFuture<'transport, Result<(), ChMuxError<TransportSinkError, TransportStreamError>>>,
);

impl<'transport, TransportSinkError, TransportStreamError>
    Connect<'transport, TransportSinkError, TransportStreamError>
{
    /// Establishes a connection over a framed transport (a [sink](Sink) and a [stream](Stream) of binary data) and
    /// returns a remote [sender](base::Sender) and [receiver](base::Receiver).
    ///
    /// This establishes a [chmux] connection over the transport and opens a remote channel.
    ///
    /// You must poll the returned [Connect] future or spawn it for the connection to work.
    ///
    /// # Panics
    /// Panics if the chmux configuration is invalid.
    pub async fn framed<TransportSink, TransportStream, T, Codec>(
        chmux_cfg: chmux::Cfg, transport_sink: TransportSink, transport_stream: TransportStream,
    ) -> Result<
        (
            Connect<'transport, TransportSinkError, TransportStreamError>,
            base::Sender<T, Codec>,
            base::Receiver<T, Codec>,
        ),
        ConnectError<TransportSinkError, TransportStreamError>,
    >
    where
        TransportSink: Sink<Bytes, Error = TransportSinkError> + Send + Sync + Unpin + 'transport,
        TransportSinkError: Error + Send + Sync + 'static,
        TransportStream: Stream<Item = Result<Bytes, TransportStreamError>> + Send + Sync + Unpin + 'transport,
        TransportStreamError: Error + Send + Sync + 'static,
        T: RemoteSend,
        Codec: codec::Codec,
    {
        let (mux, client, mut listener) = ChMux::new(chmux_cfg, transport_sink, transport_stream).await?;
        let mut connection = Self(mux.run().boxed());

        tokio::select! {
            biased;
            Err(err) = &mut connection => Err(err.into()),
            result = base::connect(&client, &mut listener) => {
                match result {
                    Ok((tx, rx)) => Ok((connection, tx, rx)),
                    Err(err) => Err(err.into()),
                }
            }
        }
    }
}

impl<'transport> Connect<'transport, io::Error, io::Error> {
    /// Establishes a connection over an IO transport (an [AsyncRead] and [AsyncWrite]) and
    /// returns a remote [sender](base::Sender) and [receiver](base::Receiver).
    ///
    /// A [chmux] connection is established over the transport and a remote channel is opened.
    /// This prepends a length header to each chmux packet for transportation over the unframed connection.
    ///
    /// This method performs no buffering of read and writes and thus may exhibit suboptimal
    /// performance if the underlying reader and writer are unbuffered.
    /// In this case use [io_buffered](Self::io_buffered) instead.
    ///
    /// You must poll the returned [Connect] future or spawn it for the connection to work.
    ///
    /// # Panics
    /// Panics if the chmux configuration is invalid.
    pub async fn io<Read, Write, T, Codec>(
        chmux_cfg: chmux::Cfg, input: Read, output: Write,
    ) -> Result<
        (Connect<'transport, io::Error, io::Error>, base::Sender<T, Codec>, base::Receiver<T, Codec>),
        ConnectError<io::Error, io::Error>,
    >
    where
        Read: AsyncRead + Send + Sync + Unpin + 'transport,
        Write: AsyncWrite + Send + Sync + Unpin + 'transport,
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
        Self::framed(chmux_cfg, transport_sink, transport_stream).await
    }

    /// Establishes a buffered connection over an IO transport (an [AsyncRead] and [AsyncWrite]) and
    /// returns a remote [sender](base::Sender) and [receiver](base::Receiver).
    ///
    /// A [chmux] connection is established over the transport and a remote channel is opened.
    /// This prepends a length header to each chmux packet for transportation over the unframed connection.
    ///
    /// This method performs internal buffering of reads and writes.
    ///
    /// You must poll the returned [Connect] future or spawn it for the connection to work.
    ///
    /// # Panics
    /// Panics if the chmux configuration is invalid.
    pub async fn io_buffered<Read, Write, T, Codec>(
        chmux_cfg: chmux::Cfg, input: Read, output: Write, buffer: usize,
    ) -> Result<
        (Connect<'transport, io::Error, io::Error>, base::Sender<T, Codec>, base::Receiver<T, Codec>),
        ConnectError<io::Error, io::Error>,
    >
    where
        Read: AsyncRead + Send + Sync + Unpin + 'transport,
        Write: AsyncWrite + Send + Sync + Unpin + 'transport,
        T: RemoteSend,
        Codec: codec::Codec,
    {
        let buf_input = BufReader::with_capacity(buffer, input);
        let buf_output = BufWriter::with_capacity(buffer, output);
        Self::io(chmux_cfg, buf_input, buf_output).await
    }
}

impl<'transport, TransportSinkError, TransportStreamError> Future
    for Connect<'transport, TransportSinkError, TransportStreamError>
{
    /// Result of connection after it has been terminated.
    type Output = Result<(), ChMuxError<TransportSinkError, TransportStreamError>>;

    /// This future runs the dispatcher for this connection.
    fn poll(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
        Pin::into_inner(self).0.poll_unpin(cx)
    }
}
