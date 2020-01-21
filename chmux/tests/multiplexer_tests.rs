use futures::prelude::*;
use futures::channel::mpsc;
use futures::stream::{Stream, StreamExt};
use futures::executor;

use chmux;


#[test]
fn simple_test() {
    let pool = executor::ThreadPool::new().unwrap();

    let queue_length = 10;
    let (a_tx, b_rx) = mpsc::channel(queue_length);
    let (b_tx, a_rx) = mpsc::channel(queue_length);

    let a_rx = a_rx.map(|v| Ok::<_, ()>(v));
    let b_rx = b_rx.map(|v| Ok::<_, ()>(v));    

    let mux_cfg = chmux::Cfg { 
        channel_rx_queue_length: 100,
        service_request_queue_length: 10,
    };

    let (a_mux, a_client, a_server) = chmux::Multiplexer::new(mux_cfg.clone(), a_tx, a_rx);
    let (b_mux, b_client, b_server) = chmux::Multiplexer::new(mux_cfg.clone(), b_tx, b_rx);

    pool.spawn_ok(async move { a_mux.run().await.unwrap() } );
    pool.spawn_ok(async move { b_mux.run().await.unwrap() } );

    executor::block_on(async move {
        let (tx, rx) = a_client.connect("abc".to_string()).await.unwrap();
    });
}

