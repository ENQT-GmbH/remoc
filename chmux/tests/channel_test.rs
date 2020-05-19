use futures::{channel::mpsc, channel::oneshot, executor, prelude::*, stream::StreamExt};
use std::io;

use chmux::{
    self,
    codecs::json::{JsonContentCodec, JsonTransportCodec},
};

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

    let (a_mux, a_client, a_server) =
        chmux::Multiplexer::new(&mux_cfg, &content_codec, &transport_codec, a_tx, a_rx);
    let (b_mux, b_client, mut b_server) =
        chmux::Multiplexer::new(&mux_cfg, &content_codec, &transport_codec, b_tx, b_rx);

    let (a_mux_done_tx, a_mux_done_rx) = oneshot::channel();
    pool.spawn_ok(async move {
        println!("A mux start");
        a_mux.run().await.unwrap();
        let _ = a_mux_done_tx.send(());
    });

    let (b_mux_done_tx, b_mux_done_rx) = oneshot::channel();
    pool.spawn_ok(async move {
        println!("B mux start");
        b_mux.run().await.unwrap();
        let _ = b_mux_done_tx.send(());
    });

    let (server_done_tx, server_done_rx) = oneshot::channel();
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
        drop(b_client);
        let _ = server_done_tx.send(());
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

        drop(tx);
        drop(rx);
        drop(a_client);

        println!("Waiting for server");
        server_done_rx.await.unwrap();

        drop(a_server);

        println!("Waiting for muxes");
        a_mux_done_rx.await.unwrap();
        b_mux_done_rx.await.unwrap();
    });
}

#[test]
fn hangup_test() {
    let pool = executor::ThreadPool::new().unwrap();

    let queue_length = 10;
    let (a_tx, b_rx) = mpsc::channel::<Vec<u8>>(queue_length);
    let (b_tx, a_rx) = mpsc::channel::<Vec<u8>>(queue_length);

    let a_rx = a_rx.map(|v| Ok::<_, io::Error>(v));
    let b_rx = b_rx.map(|v| Ok::<_, io::Error>(v));

    let mux_cfg = chmux::Cfg::default();
    let content_codec = JsonContentCodec::new();
    let transport_codec = JsonTransportCodec::new();

    let (a_mux, a_client, a_server) =
        chmux::Multiplexer::new(&mux_cfg, &content_codec, &transport_codec, a_tx, a_rx);
    let (b_mux, b_client, mut b_server) =
        chmux::Multiplexer::new(&mux_cfg, &content_codec, &transport_codec, b_tx, b_rx);

    let (a_mux_done_tx, a_mux_done_rx) = oneshot::channel();
    pool.spawn_ok(async move {
        println!("A mux start");
        a_mux.run().await.unwrap();
        let _ = a_mux_done_tx.send(());
    });

    let (b_mux_done_tx, b_mux_done_rx) = oneshot::channel();
    pool.spawn_ok(async move {
        println!("B mux start");
        b_mux.run().await.unwrap();
        let _ = b_mux_done_tx.send(());
    });

    let (server_done_tx, server_done_rx) = oneshot::channel();
    pool.spawn_ok(async move {
        println!("B server start");
        loop {
            match b_server.next().await {
                Some((service, req)) => {
                    let service: u64 = service.unwrap();
                    println!("Server connection request: {}", &service);
                    if service == 123 {
                        let (mut tx, mut rx): (chmux::Sender<String>, chmux::Receiver<usize>) =
                            req.accept().await;
                        loop {
                            match rx.next().await {
                                Some(Ok(msg)) => {
                                    println!("Server received: {}", msg);
                                    if msg == 100 {
                                        break;
                                    }
                                }
                                Some(Err(err)) => {
                                    println!("Server receive error: {:?}", err);
                                    break;
                                }
                                None => break,
                            }
                        }
                        println!("Server dropping receiver");
                        drop(rx);

                        tx.send("Server 1 msg".to_string()).await.unwrap();
                        tx.send("Server 2nd message".to_string()).await.unwrap();
                    }
                    println!("Server closed connection");
                }
                None => break,
            }
        }
        println!("B Server quit");
        drop(b_client);
        let _ = server_done_tx.send(());
    });

    executor::block_on(async move {
        println!("A client connecting to service...");
        let (mut tx, mut rx): (chmux::Sender<usize>, chmux::Receiver<String>) =
            a_client.connect(123).await.unwrap();
        println!("A client connected.");

        let hn = tx.hangup_notify();

        let mut i = 0;
        loop {
            match tx.send(i).await {
                Ok(()) => println!("A client sent: {}", i),
                Err(err) => {
                    println!("A client send error: {:?}", err);
                    break;
                }
            }
            i += 1;
        }

        println!("Waiting for hangup notification...");
        hn.await;

        loop {
            match rx.next().await {
                Some(Ok(msg)) => {
                    println!("Client received: {}", msg);
                }
                Some(Err(err)) => {
                    println!("Client receive error: {:?}", err);
                    break;
                }
                None => {
                    println!("Client receive end");
                    break;
                }
            }
        }

        println!("A client receiver closed");

        drop(tx);
        drop(rx);
        drop(a_client);

        println!("Waiting for server close");
        server_done_rx.await.unwrap();

        drop(a_server);

        println!("Waiting for muxes");
        a_mux_done_rx.await.unwrap();
        b_mux_done_rx.await.unwrap();
    });
}
