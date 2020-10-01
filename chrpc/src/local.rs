//! Process-local communication.

use futures::{channel::mpsc, pin_mut, stream::StreamExt, Future, FutureExt};
use serde::{de::DeserializeOwned, Serialize};
use std::convert::Infallible;

use chmux::{codec::id::IdTransportCodec, ContentCodecFactory, MultiplexMsg};

fn mux_cfg() -> chmux::Cfg {
    chmux::Cfg { ping_interval: None, connection_timeout: None, ..Default::default() }
}

/// An `futures::channel::mpsc` duplex channel.
pub struct MpscDuplexChannel<Content> {
    /// Send channel.
    pub tx: mpsc::Sender<MultiplexMsg<Content>>,
    /// Receive channel.
    pub rx: mpsc::Receiver<MultiplexMsg<Content>>,
}

/// A pair of duplex channels.
pub struct MpscDuplexChannelPair<Content> {
    /// Client duplex channels.
    pub client_ch: MpscDuplexChannel<Content>,
    /// Server duplex channels.
    pub server_ch: MpscDuplexChannel<Content>,
}

impl<Content> MpscDuplexChannelPair<Content> {
    /// Creates a new duplex channel pair.
    pub fn new() -> Self {
        let (server_tx, client_rx) = mpsc::channel(mux_cfg().channel_rx_queue_length);
        let (client_tx, server_rx) = mpsc::channel(mux_cfg().channel_rx_queue_length);
        Self {
            client_ch: MpscDuplexChannel { tx: client_tx, rx: client_rx },
            server_ch: MpscDuplexChannel { tx: server_tx, rx: server_rx },
        }
    }
}

/// Connects to an RPC server listening on an `futures::channel::mpsc` send and receive channel pair.
///
/// Use `<RPCClient>::bind(mpsc_client(...).await?)` to obtain an RPC client.
pub async fn mpsc_client<Service, Content, ContentCodec>(
    ch: MpscDuplexChannel<Content>, content_codec: ContentCodec,
) -> chmux::Client<Service, Content, ContentCodec>
where
    Service: Serialize + DeserializeOwned + 'static,
    Content: Serialize + DeserializeOwned + Send + 'static,
    ContentCodec: ContentCodecFactory<Content> + 'static,
{
    let MpscDuplexChannel { tx, rx } = ch;
    let rx = rx.map(|msg| Ok::<_, Infallible>(msg));

    let transport_codec = IdTransportCodec::new();

    let (mux, client, _) = chmux::Multiplexer::new(&mux_cfg(), &content_codec, &transport_codec, tx, rx);
    tokio::spawn(async move {
        if let Err(err) = mux.run().await {
            log::warn!("Client chmux failed: {}", &err);
        }
    });

    client
}

/// Runs an RPC server on an `futures::channel::mpsc` send and receive channel pair.
///
/// `run_server` takes the server mux as argument.
pub async fn mpsc_server<Service, Content, ContentCodec, ServerFut, ServerFutOutput>(
    ch: MpscDuplexChannel<Content>, content_codec: ContentCodec,
    run_server: impl Fn(chmux::Server<Service, Content, ContentCodec>) -> ServerFut + Send + Clone + 'static,
) -> ServerFutOutput
where
    Service: Serialize + DeserializeOwned + 'static,
    Content: Serialize + DeserializeOwned + Send + 'static,
    ContentCodec: ContentCodecFactory<Content> + 'static,
    ServerFut: Future<Output = ServerFutOutput> + Send,
{
    let MpscDuplexChannel { tx, rx } = ch;
    let rx = rx.map(|msg| Ok::<_, Infallible>(msg));

    let transport_codec = IdTransportCodec::new();

    let (mux, _, server_mux) = chmux::Multiplexer::new(&mux_cfg(), &content_codec, &transport_codec, tx, rx);
    let mux_task = mux.run().fuse();
    let rpc_task = run_server(server_mux);

    pin_mut!(mux_task, rpc_task);
    loop {
        tokio::select! {
            mux_result = &mut mux_task => {
                if let Err(err) = mux_result {
                    log::warn!("Server chmux failed: {}", &err);
                }
            },
            rpc_result = &mut rpc_task => return rpc_result,
        }
    }
}
