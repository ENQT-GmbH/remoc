use futures::{try_join, StreamExt};
use remoc::{
    codec::JsonCodec,
    rch::{remote, RemoteSend},
};
use std::{net::Ipv4Addr, sync::Once};
use tokio::net::{TcpListener, TcpStream};

mod chmux;
mod codec;
mod rsync;

static INIT: Once = Once::new();

pub fn init() {
    INIT.call_once(env_logger::init);
}

#[macro_export]
macro_rules! loop_transport {
    ($queue_length:expr, $a_tx:ident, $a_rx:ident, $b_tx:ident, $b_rx:ident) => {
        let ($a_tx, $b_rx) = futures::channel::mpsc::channel::<bytes::Bytes>($queue_length);
        let ($b_tx, $a_rx) = futures::channel::mpsc::channel::<bytes::Bytes>($queue_length);

        let $a_rx = $a_rx.map(Ok::<_, std::io::Error>);
        let $b_rx = $b_rx.map(Ok::<_, std::io::Error>);
    };
}

pub async fn loop_channel<T>() -> (
    (remote::Sender<T, JsonCodec>, remote::Receiver<T, JsonCodec>),
    (remote::Sender<T, JsonCodec>, remote::Receiver<T, JsonCodec>),
)
where
    T: RemoteSend,
{
    let cfg = remoc::chmux::Cfg::default();
    loop_channel_with_cfg(cfg).await
}

pub async fn loop_channel_with_cfg<T>(
    cfg: remoc::chmux::Cfg,
) -> (
    (remote::Sender<T, JsonCodec>, remote::Receiver<T, JsonCodec>),
    (remote::Sender<T, JsonCodec>, remote::Receiver<T, JsonCodec>),
)
where
    T: RemoteSend,
{
    loop_transport!(0, transport_a_tx, transport_a_rx, transport_b_tx, transport_b_rx);
    try_join!(
        remoc::connect_framed(cfg.clone(), transport_a_tx, transport_a_rx),
        remoc::connect_framed(cfg, transport_b_tx, transport_b_rx),
    )
    .expect("creating remote loop channel failed")
}

pub async fn tcp_loop_channel<T>(
    tcp_port: u16,
) -> (
    (remote::Sender<T, JsonCodec>, remote::Receiver<T, JsonCodec>),
    (remote::Sender<T, JsonCodec>, remote::Receiver<T, JsonCodec>),
)
where
    T: RemoteSend,
{
    let server = async move {
        let listener = TcpListener::bind((Ipv4Addr::new(127, 0, 0, 1), tcp_port)).await.unwrap();
        let (socket, _) = listener.accept().await.unwrap();
        let (socket_rx, socket_tx) = socket.into_split();
        remoc::connect_io(Default::default(), socket_rx, socket_tx).await
    };

    let client = async move {
        let socket = TcpStream::connect((Ipv4Addr::new(127, 0, 0, 1), tcp_port)).await.unwrap();
        let (socket_rx, socket_tx) = socket.into_split();
        remoc::connect_io(Default::default(), socket_rx, socket_tx).await
    };

    try_join!(server, client).expect("creating remote TCP loop channel failed")
}
