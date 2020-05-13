use futures::{sink::SinkExt, stream::StreamExt};
use std::{net::Ipv4Addr, time::Duration};
use tokio::{
    io::split,
    net::{TcpListener, TcpStream},
    runtime::Runtime,
};
use tokio_util::codec::{length_delimited::LengthDelimitedCodec, FramedRead, FramedWrite};

use chmux::{
    self,
    codecs::json::{JsonContentCodec, JsonTransportCodec},
};

fn tcp_server() {
    let mut rt = Runtime::new().unwrap();
    rt.block_on(async {
        let mut listener = TcpListener::bind((Ipv4Addr::new(127, 0, 0, 1), 9876)).await.unwrap();

        let (socket, _) = listener.accept().await.unwrap();
        let (socket_rx, socket_tx) = split(socket);
        let framed_tx = FramedWrite::new(socket_tx, LengthDelimitedCodec::new());
        let framed_rx = FramedRead::new(socket_rx, LengthDelimitedCodec::new());
        let framed_rx = framed_rx.map(|data| data.map(|b| b.freeze()));

        let mux_cfg = chmux::Cfg::default();
        let content_codec = JsonContentCodec::new();
        let transport_codec = JsonTransportCodec::new();

        let (mux, _, server) =
            chmux::Multiplexer::new(&mux_cfg, &content_codec, &transport_codec, framed_tx, framed_rx);
        let mut server: chmux::Server<String, _, _> = server;

        let mux_run = tokio::spawn(async move { mux.run().await.unwrap() });

        loop {
            match server.next().await {
                Some((service, req)) => {
                    let service = service.unwrap();
                    println!("Server accepting service request {}", &service);
                    let (mut tx, mut rx): (chmux::Sender<String>, chmux::Receiver<String>) = req.accept().await;
                    println!("Server accepted service request.");

                    tx.send("Hi from server".to_string()).await.unwrap();
                    println!("Server sent Hi message");

                    println!("Server dropping sender");
                    drop(tx);
                    println!("Server dropped sender");

                    loop {
                        match rx.next().await {
                            Some(msg) => {
                                let msg = msg.unwrap();
                                println!("Server received: {}", &msg);
                            }
                            None => break,
                        }
                    }
                }
                None => break,
            }
        }

        println!("Waiting for server mux to terminate...");
        mux_run.await.unwrap();
    });
}

fn tcp_client() {
    let mut rt = Runtime::new().unwrap();
    rt.block_on(async {
        let socket = TcpStream::connect((Ipv4Addr::new(127, 0, 0, 1), 9876)).await.unwrap();

        let (socket_rx, socket_tx) = split(socket);
        let framed_tx = FramedWrite::new(socket_tx, LengthDelimitedCodec::new());
        let framed_rx = FramedRead::new(socket_rx, LengthDelimitedCodec::new());
        let framed_rx = framed_rx.map(|data| data.map(|b| b.freeze()));

        let mux_cfg = chmux::Cfg::default();
        let content_codec = JsonContentCodec::new();
        let transport_codec = JsonTransportCodec::new();

        let (mux, client, _) =
            chmux::Multiplexer::new(&mux_cfg, &content_codec, &transport_codec, framed_tx, framed_rx);
        let mux_run = tokio::spawn(async move { mux.run().await.unwrap() });

        {
            let client: chmux::Client<String, _, _> = client;

            println!("Client connecting to TestService...");
            let (mut tx, mut rx) = client.connect("TestService".to_string()).await.unwrap();
            println!("Client connected");

            tx.send("Hi from client".to_string()).await.unwrap();
            println!("Client sent Hi message");

            println!("Client dropping sender");
            drop(tx);
            println!("Client dropped sender");

            loop {
                match rx.next().await {
                    Some(msg) => {
                        let msg: String = msg.unwrap();
                        println!("Client received: {}", &msg);
                    }
                    None => break,
                }
            }

            println!("Client closing connection...");
        }

        println!("Waiting for client mux to terminate...");
        mux_run.await.unwrap();
    });
}

#[test]
fn tcp_test() {
    env_logger::init();

    println!("Starting server thread...");
    let server_thread = std::thread::spawn(tcp_server);
    std::thread::sleep(Duration::from_millis(100));

    println!("String client thread...");
    let client_thread = std::thread::spawn(tcp_client);

    println!("Waiting for server thread...");
    server_thread.join().unwrap();
    println!("Waiting for client thread...");
    client_thread.join().unwrap();
}
