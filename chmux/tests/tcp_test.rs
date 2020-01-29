use std::net::Ipv4Addr;
use tokio::io::split;
use tokio::runtime::Runtime;
use tokio::net::TcpListener;
use tokio_util::codec::{FramedRead, FramedWrite};
use tokio_util::codec::length_delimited::LengthDelimitedCodec;
use tokio_serde::SymmetricallyFramed;
use tokio_serde::formats::SymmetricalJson;

use chmux;
use chmux::codecs::json::{JsonContentCodec, JsonTransportCodec};


fn tcp_server() {
    let mut rt = Runtime::new().unwrap();
    rt.block_on(async {

        let mut listener = TcpListener::bind((Ipv4Addr::new(127,0,0,1), 9876)).await.unwrap();

        let (socket, _) = listener.accept().await.unwrap();
        let (socket_rx, socket_tx) = split(socket);
        let framed_tx = FramedWrite::new(socket_tx, LengthDelimitedCodec::new());
        let framed_rx = FramedRead::new(socket_rx, LengthDelimitedCodec::new());
        let msg_tx = SymmetricallyFramed::new(framed_tx, SymmetricalJson::default());
        let msg_rx = SymmetricallyFramed::new(framed_rx, SymmetricalJson::default());

        let mux_cfg = chmux::Cfg::default();
        let content_codec = JsonContentCodec::new();
        let transport_codec = JsonTransportCodec::new();

        let (mux, _, mut server) = 
            chmux::Multiplexer::new(&mux_cfg, &content_codec, &transport_codec, msg_tx, msg_rx);
        let mut server: chmux::Server<String, _, _> = server;

        // loop {
        //     match server.next().await {
        //         Some((service, req)) => {
        //             let (mut tx, mut rx) = req.accept().await.split();
        //             tx.send("Hi".to_string()).await.unwrap();
        //         }
        //     }
        // }
    });
}



fn tcp_client() {
    
}


#[test]
fn tcp_test() {
    let server_thread = std::thread::spawn(tcp_server);
    let client_thread = std::thread::spawn(tcp_client);

    println!("Waiting for server thread...");
    server_thread.join().unwrap();
    println!("Waiting for client thread...");
    client_thread.join().unwrap();
}

