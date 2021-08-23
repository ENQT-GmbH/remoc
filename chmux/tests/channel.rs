use bytes::Bytes;
use chmux::{PortsExhausted, SendError};
use futures::{
    channel::{mpsc, oneshot},
    future::try_join,
    stream::StreamExt,
};
use std::{
    io,
    num::{NonZeroU16, NonZeroU32, NonZeroUsize},
    sync::Once,
    time::Duration,
};
use tokio::time::sleep;

static INIT: Once = Once::new();

fn init() {
    INIT.call_once(env_logger::init);
}

fn cfg(trace_id: &str) -> chmux::Cfg {
    chmux::Cfg {
        connection_timeout: Some(Duration::from_secs(1)),
        max_ports: NonZeroU32::new(20).unwrap(),
        ports_exhausted: PortsExhausted::Fail,
        max_data_size: 1_000_000,
        max_received_ports: 100,
        chunk_size: 9,
        receive_buffer: 4,
        shared_send_queue: NonZeroU16::new(3).unwrap(),
        connect_queue: NonZeroU16::new(2).unwrap(),
        trace_id: Some(trace_id.to_string()),
    }
}

fn cfg2(trace_id: &str) -> chmux::Cfg {
    chmux::Cfg {
        connection_timeout: Some(Duration::from_secs(1)),
        max_ports: NonZeroU32::new(20).unwrap(),
        ports_exhausted: PortsExhausted::Wait(Some(Duration::from_secs(5))),
        max_data_size: 1_000_000,
        max_received_ports: 100,
        chunk_size: 4,
        receive_buffer: 4,
        shared_send_queue: NonZeroU16::new(1).unwrap(),
        connect_queue: NonZeroU16::new(1).unwrap(),
        trace_id: Some(trace_id.to_string()),
    }
}

#[tokio::test]
async fn basic() {
    init();

    let queue_length = 0;
    let (a_tx, b_rx) = mpsc::channel::<Bytes>(queue_length);
    let (b_tx, a_rx) = mpsc::channel::<Bytes>(queue_length);

    let a_rx = a_rx.map(Ok::<_, io::Error>);
    let b_rx = b_rx.map(Ok::<_, io::Error>);

    println!("Connecting...");
    let ((a_mux, a_client, a_server), (b_mux, b_client, mut b_server)) = try_join(
        chmux::Multiplexer::new(&cfg("a_mux"), a_tx, a_rx),
        chmux::Multiplexer::new(&cfg2("b_mux"), b_tx, b_rx),
    )
    .await
    .unwrap();
    println!("Connected: a_mux={:?}, b_mux={:?}", &a_mux, &b_mux);

    let (a_mux_done_tx, a_mux_done_rx) = oneshot::channel();
    tokio::spawn(async move {
        println!("A mux run");
        a_mux.run().await.expect("a_mux");
        let _ = a_mux_done_tx.send(());
    });

    let (b_mux_done_tx, b_mux_done_rx) = oneshot::channel();
    tokio::spawn(async move {
        println!("B mux run");
        b_mux.run().await.expect("b_mux");
        let _ = b_mux_done_tx.send(());
    });

    const N_MSG: usize = 500;

    let (server_done_tx, server_done_rx) = oneshot::channel();
    tokio::spawn(async move {
        println!("B server start");
        while let Some((mut tx, mut rx)) = b_server.accept().await.unwrap() {
            tokio::spawn(async move {
                while let Some(msg) = rx.recv().await.unwrap() {
                    println!("Server received: {}", String::from_utf8(msg.into()).unwrap());
                }
                println!("Server receiver ended");
            });

            for i in 0..N_MSG {
                let msg = format!("message no {}", i);
                match tx.send(msg.clone().into()).await {
                    Ok(()) => (),
                    Err(SendError::Closed { gracefully }) => {
                        println!("Client closed connection with gracefully={}", gracefully);
                        break;
                    }
                    other => other.unwrap(),
                }
                println!("Server sent: {}", msg);
            }

            drop(tx);
            println!("Server dropped transmitter");
        }

        println!("B Server quit");
        drop(b_client);
        let _ = server_done_tx.send(());
    });

    {
        println!("A client connecting 1...");
        let ret: Result<(chmux::Sender, chmux::Receiver), _> = a_client.connect().await;
        println!("A client connect result: {:?}", &ret);
    }

    println!("Delay test...");
    sleep(Duration::from_secs(3)).await;

    println!("A client connecting 2...");
    let (mut tx, mut rx): (chmux::Sender, chmux::Receiver) = a_client.connect().await.unwrap();
    println!("A client connected.");

    let mut n_recv = 0;
    while let Some(msg) = rx.recv().await.unwrap() {
        let s = String::from_utf8(msg.into()).unwrap();
        println!("A client received: {}", &s);
        n_recv += 1;

        println!("A client replying...");
        tx.send(format!("Reply: {}", &s).into()).await.unwrap();
        println!("A client replied");
    }
    if n_recv != N_MSG {
        panic!("received not equal number messages than sent");
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
}

#[tokio::test(flavor = "multi_thread", worker_threads = 8)]
async fn hangup() {
    init();

    let queue_length = 10;
    let (a_tx, b_rx) = mpsc::channel::<Bytes>(queue_length);
    let (b_tx, a_rx) = mpsc::channel::<Bytes>(queue_length);

    let a_rx = a_rx.map(Ok::<_, io::Error>);
    let b_rx = b_rx.map(Ok::<_, io::Error>);

    println!("Connecting...");
    let ((a_mux, a_client, a_server), (b_mux, b_client, mut b_server)) = try_join(
        chmux::Multiplexer::new(&cfg("a_mux"), a_tx, a_rx),
        chmux::Multiplexer::new(&cfg2("b_mux"), b_tx, b_rx),
    )
    .await
    .unwrap();
    println!("Connected: a_mux={:?}, b_mux={:?}", &a_mux, &b_mux);

    let (a_mux_done_tx, a_mux_done_rx) = oneshot::channel();
    tokio::spawn(async move {
        println!("A mux start");
        a_mux.run().await.unwrap();
        let _ = a_mux_done_tx.send(());
    });

    let (b_mux_done_tx, b_mux_done_rx) = oneshot::channel();
    tokio::spawn(async move {
        println!("B mux start");
        b_mux.run().await.unwrap();
        let _ = b_mux_done_tx.send(());
    });

    let (server_done_tx, server_done_rx) = oneshot::channel();
    tokio::spawn(async move {
        println!("B server start");
        while let Some((mut tx, mut rx)) = b_server.accept().await.unwrap() {
            while let Some(msg) = rx.recv().await.unwrap() {
                let msg = Vec::from(msg);
                println!("Server received: {}", msg[0]);
                if msg[0] == 100 {
                    break;
                }
            }
            println!("Server dropping receiver");
            drop(rx);

            tx.send("Server 1 msg".into()).await.unwrap();
            tx.send("Server 2nd message".into()).await.unwrap();

            println!("Server closed connection");
        }
        println!("B Server quit");
        drop(b_client);
        let _ = server_done_tx.send(());
    });

    println!("A client connecting to service...");
    let (mut tx, mut rx): (chmux::Sender, chmux::Receiver) = a_client.connect().await.unwrap();
    println!("A client connected.");

    let hn = tx.closed();

    let mut i = 0u16;
    loop {
        let msg = vec![i as u8];
        match tx.send(msg.into()).await {
            Ok(()) => println!("A client sent: {}", i),
            Err(err) => {
                println!("A client send error: {:?}", err);
                break;
            }
        }
        i += 1;
    }
    if i <= 100 {
        panic!("Send error too early");
    }

    println!("Waiting for hangup notification...");
    hn.await;

    loop {
        match rx.recv().await.unwrap() {
            Some(msg) => {
                println!("Client received: {}", String::from_utf8(msg.into()).unwrap());
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
}
