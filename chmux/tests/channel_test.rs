use futures::channel::mpsc;
use futures::executor;
use futures::prelude::*;
use futures::stream::StreamExt;
use std::io;

use chmux;
use chmux::codecs::json::{JsonContentCodec, JsonTransportCodec};

#[test]
fn raw_test() {
    env_logger::init();
    let pool = executor::ThreadPool::new().unwrap();

    let queue_length = 10;
    let (a_tx, b_rx) = mpsc::channel::<Vec<u8>>(queue_length);
    let (b_tx, a_rx) = mpsc::channel::<Vec<u8>>(queue_length);

    let a_rx = a_rx.map(|v| Ok::<_, io::Error>(v));
    let b_rx = b_rx.map(|v| Ok::<_, io::Error>(v));

    let mux_cfg = chmux::Cfg::default();
    let content_codec = JsonContentCodec::new();
    let transport_codec = JsonTransportCodec::new();

    let (a_mux, mut a_client, _a_server) =
        chmux::Multiplexer::new(&mux_cfg, &content_codec, &transport_codec, a_tx, a_rx);
    let (b_mux, _b_client, mut b_server) =
        chmux::Multiplexer::new(&mux_cfg, &content_codec, &transport_codec, b_tx, b_rx);

    pool.spawn_ok(async move {
        println!("A mux start");
        a_mux.run().await.unwrap();
        println!("A mux terminated");
    });
    pool.spawn_ok(async move {
        println!("B mux start");
        b_mux.run().await.unwrap();
        println!("B mux terminated");
    });

    pool.spawn_ok(async move {
        println!("B server start");
        loop {
            match b_server.next().await {
                Some((service, req)) => {
                    let service: u64 = service.unwrap();
                    println!("Server connection request: {}", &service);
                    if service == 123 {
                        let (mut tx, mut rx): (chmux::Sender<String>, chmux::Receiver<String>) =
                            req.accept().await;
                        tx.send("Hi".to_string()).await.unwrap();
                        println!("Server sent hi");
                        tx.send("Hi2".to_string()).await.unwrap();
                        println!("Server sent hi2");

                        drop(tx);
                        println!("Server dropped transmitter");
                        loop {
                            match rx.next().await {
                                Some(msg) => {
                                    println!("Server received: {}", msg.unwrap());
                                }
                                None => break,
                            }
                        }
                    }
                    println!("Server closed connection");
                }
                None => break,
            }
        }
        println!("B Server quit");
    });

    executor::block_on(async move {
        println!("A client connecting to service 987...");
        let ret: Result<(chmux::Sender<String>, chmux::Receiver<String>), _> = a_client.connect(987u64).await;
        println!("A client connect result: {:?}", &ret);

        println!("A client connecting to service 123...");
        let (mut tx, mut rx): (chmux::Sender<String>, chmux::Receiver<String>) =
            a_client.connect(123).await.unwrap();
        println!("A client connected.");
        loop {
            match rx.next().await {
                Some(Ok(msg)) => {
                    println!("A client received: {}", &msg);
                    tx.send(format!("Reply: {}", msg)).await.unwrap();
                }
                Some(Err(err)) => {
                    println!("A client receive error: {:?}", &err);
                }
                None => break,
            }
        }
        println!("A client receiver closed");
    });
}
