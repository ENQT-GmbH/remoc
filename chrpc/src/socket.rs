//! Utility functions for establishing RPC connections over sockets.

use bytes::Bytes;
use futures::{stream::StreamExt, Future};
use pin_project::pin_project;
use serde::{de::DeserializeOwned, Serialize};
use socket2::TcpKeepalive;
use std::{
    error, io,
    net::{Ipv4Addr, SocketAddr, SocketAddrV4},
    os::unix::io::{AsRawFd, FromRawFd, IntoRawFd},
    pin::Pin,
    sync::Arc,
    task::{Context, Poll},
    time::Duration,
};
use tokio::{
    io::{split, AsyncRead, AsyncWrite, ReadBuf},
    net::{TcpListener, TcpStream, ToSocketAddrs},
};
use tokio_util::codec::{FramedRead, FramedWrite, LengthDelimitedCodec};

use chmux::{ContentCodecFactory, Multiplexer, TransportCodecFactory};

/// TCP keep alive time in seconds.
const TCP_KEEPALIVE_TIME: Duration = Duration::from_secs(10);

/// Configure TCP keep alive time on socket.
fn configure_tcp_keepalive(socket: &TcpStream) -> io::Result<()> {
    let fd = socket.as_raw_fd();
    let socket = unsafe { socket2::Socket::from_raw_fd(fd) };
    let keepalive = TcpKeepalive::new().with_time(TCP_KEEPALIVE_TIME);
    let result = socket.set_tcp_keepalive(&keepalive);
    socket.into_raw_fd();
    result
}

/// Combines an object implementing `AsyncRead` and an object implementing
/// `AsyncWrite` into an object that implements both `AsyncRead` and
/// `AsyncWrite`.
#[pin_project]
pub struct AsyncReadWrite<R, W> {
    #[pin]
    reader: R,
    #[pin]
    writer: W,
}

impl<R, W> AsyncReadWrite<R, W> {
    /// Combine reader and writer into one object.
    pub fn new(reader: R, writer: W) -> Self {
        Self { reader, writer }
    }

    /// Extract original reader and writer.
    pub fn split(self) -> (R, W) {
        let Self { reader, writer } = self;
        (reader, writer)
    }
}

impl<R: AsyncRead, W> AsyncRead for AsyncReadWrite<R, W> {
    fn poll_read(self: Pin<&mut Self>, cx: &mut Context, buf: &mut ReadBuf) -> Poll<io::Result<()>> {
        self.project().reader.poll_read(cx, buf)
    }
}

impl<R, W: AsyncWrite> AsyncWrite for AsyncReadWrite<R, W> {
    fn poll_write(self: Pin<&mut Self>, cx: &mut Context, buf: &[u8]) -> Poll<Result<usize, io::Error>> {
        self.project().writer.poll_write(cx, buf)
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Result<(), io::Error>> {
        self.project().writer.poll_flush(cx)
    }

    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Result<(), io::Error>> {
        self.project().writer.poll_shutdown(cx)
    }

    fn is_write_vectored(&self) -> bool {
        self.writer.is_write_vectored()
    }
}

/// Connects to an RPC server listening on a socket and at the same time
/// provide an RPC server implementing `AsyncRead` and `AsyncWrite`.
pub async fn client_server<Socket, ClientService, ServerService, Content, ContentCodec, TransportCodec>(
    socket: Socket, content_codec: ContentCodec, transport_codec: TransportCodec, mux_cfg: chmux::Cfg,
) -> (chmux::Client<ClientService, Content, ContentCodec>, chmux::Server<ServerService, Content, ContentCodec>)
where
    Socket: AsyncRead + AsyncWrite + Send + 'static,
    ClientService: Serialize + DeserializeOwned + 'static,
    ServerService: Serialize + DeserializeOwned + 'static,
    Content: Serialize + DeserializeOwned + Send + 'static,
    ContentCodec: ContentCodecFactory<Content> + 'static,
    TransportCodec: TransportCodecFactory<Content, Bytes> + 'static,
{
    let (socket_rx, socket_tx) = split(socket);
    let framed_tx = FramedWrite::new(socket_tx, LengthDelimitedCodec::new());
    let framed_rx = FramedRead::new(socket_rx, LengthDelimitedCodec::new());
    let framed_rx = framed_rx.map(|data| data.map(|b| b.freeze()));

    let (mux, client, server) =
        chmux::Multiplexer::new(&mux_cfg, &content_codec, &transport_codec, framed_tx, framed_rx);
    tokio::spawn(async move {
        if let Err(err) = mux.run().await {
            log::warn!("Client and server chmux failed: {}", &err);
        }
    });

    (client, server)
}

/// Connects to an RPC server listening on a socket implementing `AsyncRead` and `AsyncWrite`.
///
/// Use `<RPCClient>::bind(client(...).await)` to obtain an RPC client.
pub async fn client<Socket, Service, Content, ContentCodec, TransportCodec>(
    socket: Socket, content_codec: ContentCodec, transport_codec: TransportCodec, mux_cfg: chmux::Cfg,
) -> chmux::Client<Service, Content, ContentCodec>
where
    Socket: AsyncRead + AsyncWrite + Send + 'static,
    Service: Serialize + DeserializeOwned + 'static,
    Content: Serialize + DeserializeOwned + Send + 'static,
    ContentCodec: ContentCodecFactory<Content> + 'static,
    TransportCodec: TransportCodecFactory<Content, Bytes> + 'static,
{
    let (socket_rx, socket_tx) = split(socket);
    let framed_tx = FramedWrite::new(socket_tx, LengthDelimitedCodec::new());
    let framed_rx = FramedRead::new(socket_rx, LengthDelimitedCodec::new());
    let framed_rx = framed_rx.map(|data| data.map(|b| b.freeze()));

    let (mux, client, _) =
        chmux::Multiplexer::new::<_, ()>(&mux_cfg, &content_codec, &transport_codec, framed_tx, framed_rx);
    tokio::spawn(async move {
        if let Err(err) = mux.run().await {
            log::warn!("Client chmux failed: {}", &err);
        }
    });

    client
}

/// Provides an RPC server listening on a socket implementing `AsyncRead` and `AsyncWrite`.
///
/// Use `<RPCServer>::serve(server(...).await)` to run an RPC server.
pub async fn server<Socket, Service, Content, ContentCodec, TransportCodec>(
    socket: Socket, content_codec: ContentCodec, transport_codec: TransportCodec, mux_cfg: chmux::Cfg,
) -> chmux::Server<Service, Content, ContentCodec>
where
    Socket: AsyncRead + AsyncWrite + Send + 'static,
    Service: Serialize + DeserializeOwned + 'static,
    Content: Serialize + DeserializeOwned + Send + 'static,
    ContentCodec: ContentCodecFactory<Content> + 'static,
    TransportCodec: TransportCodecFactory<Content, Bytes> + 'static,
{
    let (socket_rx, socket_tx) = split(socket);
    let framed_tx = FramedWrite::new(socket_tx, LengthDelimitedCodec::new());
    let framed_rx = FramedRead::new(socket_rx, LengthDelimitedCodec::new());
    let framed_rx = framed_rx.map(|data| data.map(|b| b.freeze()));

    let (mux, _, server) =
        Multiplexer::new::<(), _>(&mux_cfg, &content_codec, &transport_codec, framed_tx, framed_rx);
    tokio::spawn(async move {
        if let Err(err) = mux.run().await {
            log::warn!("Server chmux failed: {}", &err);
        }
    });

    server
}

/// Connects to an RPC server listening on a TCP socket.
///
/// Use `<RPCClient>::bind(tcp_client(...).await?)` to obtain an RPC client.
pub async fn tcp_client<Service, Content, ContentCodec, TransportCodec>(
    server_addr: impl ToSocketAddrs, content_codec: ContentCodec, transport_codec: TransportCodec,
    mux_cfg: chmux::Cfg,
) -> io::Result<chmux::Client<Service, Content, ContentCodec>>
where
    Service: Serialize + DeserializeOwned + 'static,
    Content: Serialize + DeserializeOwned + Send + 'static,
    ContentCodec: ContentCodecFactory<Content> + 'static,
    TransportCodec: TransportCodecFactory<Content, Bytes> + 'static,
{
    let socket = TcpStream::connect(server_addr).await?;
    let _ = configure_tcp_keepalive(&socket);

    let local_addr = socket.local_addr().unwrap_or_else(|_| SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, 0).into());
    let remote_addr = socket.peer_addr().unwrap_or_else(|_| SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, 0).into());
    log::info!("Connected from {} to {}", &local_addr, &remote_addr);

    Ok(client(socket, content_codec, transport_codec, mux_cfg).await)
}

/// Listens on the specified TCP socket and runs an RPC server for each incoming connection.
///
/// If `multiple` is `false` only a single connection is accepted at a time.
/// `run_server` takes the local address, remote address and server mux as arguments.
pub async fn tcp_server<Service, Content, ContentCodec, TransportCodec, ServerFut, ServerFutOk, ServerFutErr>(
    bind_addr: impl ToSocketAddrs, multiple: bool, content_codec: ContentCodec, transport_codec: TransportCodec,
    mux_cfg: chmux::Cfg,
    run_server: impl Fn(SocketAddr, SocketAddr, chmux::Server<Service, Content, ContentCodec>) -> ServerFut
        + Send
        + Sync
        + Clone
        + 'static,
) -> io::Result<()>
where
    Service: Serialize + DeserializeOwned + 'static,
    Content: Serialize + DeserializeOwned + Send + 'static,
    ContentCodec: ContentCodecFactory<Content> + 'static,
    TransportCodec: TransportCodecFactory<Content, Bytes> + 'static,
    ServerFut: Future<Output = Result<ServerFutOk, ServerFutErr>> + Send,
    ServerFutErr: error::Error,
{
    let listener = TcpListener::bind(bind_addr).await?;

    loop {
        if let Ok((socket, remote_addr)) = listener.accept().await {
            let local_addr =
                socket.local_addr().unwrap_or_else(|_| SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, 0).into());
            let conn_content_codec = content_codec.clone();
            let conn_transport_codec = transport_codec.clone();
            let conn_run_server = run_server.clone();
            let conn_mux_cfg = mux_cfg.clone();

            let conn_task = async move {
                log::info!("Accepted connection for {} from {}", &local_addr, &remote_addr);
                let _ = configure_tcp_keepalive(&socket);

                let server_mux = server(socket, conn_content_codec, conn_transport_codec, conn_mux_cfg).await;
                let result = conn_run_server(local_addr, remote_addr, server_mux).await;
                if let Err(err) = result {
                    log::warn!("RPC server for {} from {} failed: {}", &local_addr, &remote_addr, &err);
                }

                log::info!("Closed connection for {} from {}", &local_addr, &remote_addr);
            };

            if multiple {
                tokio::spawn(conn_task);
            } else {
                conn_task.await;
            }
        }
    }
}

#[cfg(feature = "socket-tls")]
/// Connects to an RPC server listening on a TCP socket with TLS encryption.
///
/// Use `<RPCClient>::bind(tcp_tls_client(...).await?)` to obtain an RPC client.
pub async fn tcp_tls_client<Service, Content, ContentCodec, TransportCodec>(
    server_addr: impl ToSocketAddrs, dns_name: &str, content_codec: ContentCodec,
    transport_codec: TransportCodec, mux_cfg: chmux::Cfg, tls_cfg: Arc<tokio_rustls::rustls::ClientConfig>,
) -> io::Result<chmux::Client<Service, Content, ContentCodec>>
where
    Service: Serialize + DeserializeOwned + 'static,
    Content: Serialize + DeserializeOwned + Send + 'static,
    ContentCodec: ContentCodecFactory<Content> + 'static,
    TransportCodec: TransportCodecFactory<Content, Bytes> + 'static,
{
    use tokio_rustls::{webpki::DNSNameRef, TlsConnector};

    let dns_name = DNSNameRef::try_from_ascii_str(dns_name)
        .map_err(|err| io::Error::new(io::ErrorKind::InvalidInput, err))?;

    let socket = TcpStream::connect(server_addr).await?;
    let _ = configure_tcp_keepalive(&socket);
    let local_addr = socket.local_addr().unwrap_or_else(|_| SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, 0).into());
    let remote_addr = socket.peer_addr().unwrap_or_else(|_| SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, 0).into());
    log::info!("Connected from {} to {}", &local_addr, &remote_addr);

    let connector = TlsConnector::from(tls_cfg);
    let socket = connector.connect(dns_name, socket).await?;

    Ok(client(socket, content_codec, transport_codec, mux_cfg).await)
}

#[cfg(feature = "socket-tls")]
/// Listens on the specified TCP socket and runs an RPC server with TLS encryption for each incoming connection.
///
/// If `multiple` is `false` only a single connection is accepted at a time.
/// `run_server` takes the local address, remote address and server mux as arguments.
pub async fn tcp_tls_server<
    Service,
    Content,
    ContentCodec,
    TransportCodec,
    ServerFut,
    ServerFutOk,
    ServerFutErr,
>(
    bind_addr: impl ToSocketAddrs, multiple: bool, content_codec: ContentCodec, transport_codec: TransportCodec,
    mux_cfg: chmux::Cfg, tls_cfg: Arc<tokio_rustls::rustls::ServerConfig>,
    run_server: impl Fn(SocketAddr, SocketAddr, chmux::Server<Service, Content, ContentCodec>) -> ServerFut
        + Send
        + Sync
        + Clone
        + 'static,
) -> io::Result<()>
where
    Service: Serialize + DeserializeOwned + 'static,
    Content: Serialize + DeserializeOwned + Send + 'static,
    ContentCodec: ContentCodecFactory<Content> + 'static,
    TransportCodec: TransportCodecFactory<Content, Bytes> + 'static,
    ServerFut: Future<Output = Result<ServerFutOk, ServerFutErr>> + Send,
    ServerFutErr: error::Error,
{
    use tokio_rustls::TlsAcceptor;

    let listener = TcpListener::bind(bind_addr).await?;

    loop {
        if let Ok((socket, remote_addr)) = listener.accept().await {
            let local_addr =
                socket.local_addr().unwrap_or_else(|_| SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, 0).into());
            let conn_content_codec = content_codec.clone();
            let conn_transport_codec = transport_codec.clone();
            let conn_run_server = run_server.clone();
            let conn_mux_cfg = mux_cfg.clone();
            let conn_tls_cfg = tls_cfg.clone();

            let conn_task = async move {
                log::info!("Accepted connection for {} from {}", &local_addr, &remote_addr);
                let _ = configure_tcp_keepalive(&socket);

                let acceptor = TlsAcceptor::from(conn_tls_cfg);
                let socket = match acceptor.accept(socket).await {
                    Ok(socket) => socket,
                    Err(err) => {
                        log::warn!("TLS connection for {} from {} failed: {}", &local_addr, &remote_addr, &err);
                        return;
                    }
                };

                let server_mux = server(socket, conn_content_codec, conn_transport_codec, conn_mux_cfg).await;
                let result = conn_run_server(local_addr, remote_addr, server_mux).await;
                if let Err(err) = result {
                    log::warn!("RPC server for {} from {} failed: {}", &local_addr, &remote_addr, &err);
                }

                log::info!("Closed connection for {} from {}", &local_addr, &remote_addr);
            };

            if multiple {
                tokio::spawn(conn_task);
            } else {
                conn_task.await;
            }
        }
    }
}
