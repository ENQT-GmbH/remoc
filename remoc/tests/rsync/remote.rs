use bytes::Bytes;
use futures::{try_join, StreamExt};
use remoc::{
    chmux,
    codec::JsonCodec,
    rsync::{remote, RemoteSend},
};

use crate::loop_transport;

async fn loop_channel<T>() -> (
    (remote::Sender<T, JsonCodec>, remote::Receiver<T, JsonCodec>),
    (remote::Sender<T, JsonCodec>, remote::Receiver<T, JsonCodec>),
)
where
    T: RemoteSend,
{
    let cfg = chmux::Cfg::default();
    loop_transport!(0, transport_a_tx, transport_a_rx, transport_b_tx, transport_b_rx);
    try_join!(
        remoc::connect_framed(&cfg, transport_a_tx, transport_a_rx),
        remoc::connect_framed(&cfg, transport_b_tx, transport_b_rx),
    )
    .expect("creating remote loop channel failed")
}

#[tokio::test]
async fn remote_negation() {
    crate::init();
    let ((mut a_tx, mut a_rx), (mut b_tx, mut b_rx)) = loop_channel::<i32>().await;

    let reply_task = tokio::spawn(async move {
        while let Some(i) = b_rx.recv().await.unwrap() {
            match b_tx.send(-i).await {
                Ok(()) => (),
                Err(err) if err.is_closed() => break,
                Err(err) => panic!("reply sending failed: {}", err),
            }
        }
    });

    for i in 1..1024 {
        println!("Sending {}", i);
        a_tx.send(i).await.unwrap();
        let r = a_rx.recv().await.unwrap().unwrap();
        println!("Received {}", r);
        assert_eq!(i, -r, "wrong reply");
    }

    reply_task.await.expect("reply task failed");
}
