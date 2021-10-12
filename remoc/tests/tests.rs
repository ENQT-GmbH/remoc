use futures::{try_join, StreamExt};
use remoc::{
    codec::JsonCodec,
    rsync::{remote, RemoteSend},
};
use std::sync::Once;

mod chmux;
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
    loop_channel_with_cfg(&cfg).await
}

pub async fn loop_channel_with_cfg<T>(
    cfg: &remoc::chmux::Cfg,
) -> (
    (remote::Sender<T, JsonCodec>, remote::Receiver<T, JsonCodec>),
    (remote::Sender<T, JsonCodec>, remote::Receiver<T, JsonCodec>),
)
where
    T: RemoteSend,
{
    loop_transport!(0, transport_a_tx, transport_a_rx, transport_b_tx, transport_b_rx);
    try_join!(
        remoc::connect_framed(cfg, transport_a_tx, transport_a_rx),
        remoc::connect_framed(cfg, transport_b_tx, transport_b_rx),
    )
    .expect("creating remote loop channel failed")
}
