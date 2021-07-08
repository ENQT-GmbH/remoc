use futures::{sink::SinkExt, stream::StreamExt};
use std::net::Ipv4Addr;
use tokio::{
    io::split,
    net::{TcpListener, TcpStream},
};
use tokio_util::codec::{length_delimited::LengthDelimitedCodec, FramedRead, FramedWrite};

use chmux::{
    self,
    codec::json::{JsonCodec, JsonTransportCodec},
};
use chrpc::service;

#[service]
pub trait TestTrait {
    async fn method_one(&self, param1: u32, param2: u64, tx: chmux::Sender<String>);
    async fn method_two(&self, param1: String) -> usize;
    async fn method_three(&mut self, param1: f32, param2: f64, value: u32, rx: chmux::Receiver<u16>) -> String;
}

struct TestObj {
    value: u32,
}

impl TestObj {
    pub fn new() -> TestObj {
        TestObj { value: 123 }
    }
}

#[async_trait::async_trait]
impl TestTrait for TestObj {
    async fn method_one(&self, param1: u32, param2: u64, mut tx: chmux::Sender<String>) {
        tx.send(format!("param1={}, param2={}, value={}", param1, param2, self.value)).await.unwrap();
    }

    async fn method_two(&self, param1: String) -> usize {
        param1.len()
    }

    async fn method_three(
        &mut self, param1: f32, param2: f64, value: u32, mut rx: chmux::Receiver<u16>,
    ) -> String {
        self.value = value;
        let mut data = format!("param1={}, param2={}, value={}\n", param1, param2, value);
        println!("server: method_three: {}", &data);
        while let Some(Ok(x)) = rx.next().await {
            data.push_str(&format!("rx: {}\n", x));
            println!("server: method_three: rx: {}", x);
        }
        println!("server: method_three finished: {}", &data);
        data
    }
}

async fn server_part() {
    println!("Server waiting for TCP connection...");
    let listener = TcpListener::bind((Ipv4Addr::new(127, 0, 0, 1), 9876)).await.unwrap();
    let (socket, _) = listener.accept().await.unwrap();
    let (socket_rx, socket_tx) = split(socket);
    let framed_tx = FramedWrite::new(socket_tx, LengthDelimitedCodec::new());
    let framed_rx = FramedRead::new(socket_rx, LengthDelimitedCodec::new());
    let framed_rx = framed_rx.map(|data| data.map(|b| b.freeze()));

    let mux_cfg = chmux::Cfg::default();
    let content_codec = JsonCodec::new();
    let transport_codec = JsonTransportCodec::new();

    let (mux, _, server) =
        chmux::Multiplexer::new::<(), _>(&mux_cfg, &content_codec, &transport_codec, framed_tx, framed_rx);

    let mux_run = tokio::spawn(async move { mux.run().await.unwrap() });

    let obj = TestObj::new();
    println!("Server serving...");
    obj.serve(server).await.unwrap();
    println!("Serving ended.");

    println!("Waiting for server mux to terminate...");
    mux_run.await.unwrap();
    println!("server mux ended.");
}

async fn client_part() {
    println!("Client making TCP connection...");
    let socket = TcpStream::connect((Ipv4Addr::new(127, 0, 0, 1), 9876)).await.unwrap();
    let (socket_rx, socket_tx) = split(socket);
    let framed_tx = FramedWrite::new(socket_tx, LengthDelimitedCodec::new());
    let framed_rx = FramedRead::new(socket_rx, LengthDelimitedCodec::new());
    let framed_rx = framed_rx.map(|data| data.map(|b| b.freeze()));

    let mux_cfg = chmux::Cfg::default();
    let content_codec = JsonCodec::new();
    let transport_codec = JsonTransportCodec::new();

    let (mux, client, _) =
        chmux::Multiplexer::new::<_, ()>(&mux_cfg, &content_codec, &transport_codec, framed_tx, framed_rx);

    let mux_run = tokio::spawn(async move { mux.run().await.unwrap() });

    println!("Client making requests...");
    let mut obj_client = TestTraitClient::bind(client);
    println!("Client ended.");

    println!("Calling method_one..");
    let mut rx_one = obj_client.method_one(111, 222).await.unwrap();
    while let Some(msg) = rx_one.next().await {
        println!("rx_one: {}", msg.unwrap());
    }

    println!("Calling method_two...");
    let res_two = obj_client.method_two("1234567".to_string()).await.unwrap();
    println!("Method two: {}", res_two);

    println!("Dropping rx_one...");
    drop(rx_one);

    println!("Calling method three...");
    let mut tx_three = obj_client.method_three(11.1, 22.22, 32).await.unwrap();
    println!("Method three send 11");
    tx_three.send(11).await.unwrap();
    println!("Method three send 22");
    tx_three.send(22).await.unwrap();
    println!("Method three send 33");
    tx_three.send(33).await.unwrap();
    println!("Method three finish");
    let res_three = tx_three.finish().await.unwrap();
    println!("Method three: {}", res_three);

    //rx_one.next().await;

    println!("Dropping client...");
    drop(obj_client);
    println!("Client dropped.");

    println!("Waiting for client mux to terminate...");
    mux_run.await.unwrap();
    println!("client mux ended.");
}

#[tokio::test]
async fn test1() {
    env_logger::init();

    let server = tokio::spawn(server_part());
    let client = tokio::spawn(client_part());

    println!("Waiting for client to terminate...");
    client.await.unwrap();
    println!("Waiting for server to terminate...");
    server.await.unwrap();
}
