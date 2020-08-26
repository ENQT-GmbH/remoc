//! Utility functions for establishing RPC connections over TCP sockets.

use bytes::Bytes;
use futures::{pin_mut, stream::StreamExt, Future};
use serde::{de::DeserializeOwned, Serialize};
use std::{
    error, io,
    net::{Ipv4Addr, SocketAddr, SocketAddrV4},
    time::Duration,
};
use tokio::{
    io::split,
    net::{TcpListener, TcpStream, ToSocketAddrs},
};
use tokio_util::codec::{FramedRead, FramedWrite, LengthDelimitedCodec};

use chmux::{ContentCodecFactory, Multiplexer, TransportCodecFactory};

const TCP_KEEPALIVE_TIME: u64 = 10;

/// Connects to an RPC server listening on a TCP socket.
///
/// Use `<RPCClient>::bind(tcp_client(...).await?)` to obtain an RPC client.
pub async fn tcp_client<Service, Content, ContentCodec, TransportCodec>(
    server_addr: impl ToSocketAddrs, content_codec: ContentCodec, transport_codec: TransportCodec,
) -> io::Result<chmux::Client<Service, Content, ContentCodec>>
where
    Service: Serialize + DeserializeOwned + 'static,
    Content: Serialize + DeserializeOwned + Send + 'static,
    ContentCodec: ContentCodecFactory<Content> + 'static,
    TransportCodec: TransportCodecFactory<Content, Bytes> + 'static,
{
    let socket = TcpStream::connect(server_addr).await?;
    let _ = socket.set_keepalive(Some(Duration::from_secs(TCP_KEEPALIVE_TIME)));

    let local_addr = socket.local_addr().unwrap_or(SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, 0).into());
    let remote_addr = socket.local_addr().unwrap_or(SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, 0).into());
    log::info!("Connected from {} to {}", &local_addr, &remote_addr);

    let (socket_rx, socket_tx) = split(socket);
    let framed_tx = FramedWrite::new(socket_tx, LengthDelimitedCodec::new());
    let framed_rx = FramedRead::new(socket_rx, LengthDelimitedCodec::new());
    let framed_rx = framed_rx.map(|data| data.map(|b| b.freeze()));

    let mux_cfg = chmux::Cfg::default();
    let (mux, client, _) =
        chmux::Multiplexer::new(&mux_cfg, &content_codec, &transport_codec, framed_tx, framed_rx);
    tokio::spawn(async move {
        if let Err(err) = mux.run().await {
            log::warn!("Client chmux for {} from {} failed: {}", &remote_addr, &local_addr, &err);
        }
    });

    Ok(client)
}

/// Listens on the specified TCP socket and runs an RPC server for each incoming connection.
///
/// Uses the default chmux configuration.
/// If `multiple` is `false` only a single connection is accepted at a time.
/// `run_server` takes the local address, remote address and server mux as arguments.
pub async fn tcp_server<Service, Content, ContentCodec, TransportCodec, ServerFut, ServerFutOk, ServerFutErr>(
    bind_addr: impl ToSocketAddrs, multiple: bool, content_codec: ContentCodec, transport_codec: TransportCodec,
    run_server: impl Fn(SocketAddr, SocketAddr, chmux::Server<Service, Content, ContentCodec>) -> ServerFut
        + Send
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
    let mut listener = TcpListener::bind(bind_addr).await?;

    loop {
        if let Ok((socket, remote_addr)) = listener.accept().await {
            let local_addr = socket.local_addr().unwrap_or(SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, 0).into());
            let conn_content_codec = content_codec.clone();
            let conn_transport_codec = transport_codec.clone();
            let conn_run_server = run_server.clone();

            let conn_task = async move {
                log::info!("Accepted connection for {} from {}", &local_addr, &remote_addr);
                let _ = socket.set_keepalive(Some(Duration::from_secs(TCP_KEEPALIVE_TIME)));

                let (socket_rx, socket_tx) = split(socket);
                let framed_tx = FramedWrite::new(socket_tx, LengthDelimitedCodec::new());
                let framed_rx = FramedRead::new(socket_rx, LengthDelimitedCodec::new());
                let framed_rx = framed_rx.map(|data| data.map(|b| b.freeze()));

                let mux_cfg = chmux::Cfg::default();
                let (mux, _, server_mux) =
                    Multiplexer::new(&mux_cfg, &conn_content_codec, &conn_transport_codec, framed_tx, framed_rx);
                let mux_task = mux.run();
                let rpc_task = conn_run_server(local_addr.clone(), remote_addr.clone(), server_mux);

                pin_mut!(mux_task, rpc_task);
                tokio::select! {
                    mux_result = &mut mux_task => {
                        if let Err(err) = mux_result {
                            log::warn!("Server chmux for {} from {} failed: {}", &local_addr, &remote_addr, &err);
                        }
                    }
                    rpc_result = &mut rpc_task => {
                        if let Err(err) = rpc_result {
                            log::warn!("RPC server for {} from {} failed: {}", &local_addr, &remote_addr, &err);
                        }
                    }
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
