#[cfg(unix)]
mod unix {
    use futures::sink::SinkExt;
    use futures::stream::StreamExt;
    use std::fs;
    use std::time::Duration;
    use tokio::io::split;
    use tokio::net::{UnixListener, UnixStream};
    use tokio::runtime::Runtime;
    use tokio_util::codec::length_delimited::LengthDelimitedCodec;
    use tokio_util::codec::{FramedRead, FramedWrite};

    use chmux;
    use chmux::codecs::json::{JsonContentCodec, JsonTransportCodec};

    fn uds_server() {
        let mut rt = Runtime::new().unwrap();
        rt.block_on(async {
            let _ = fs::remove_file("/tmp/chmux_test");
            let mut listener = UnixListener::bind("/tmp/chmux_test").unwrap();

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
                        let (mut tx, mut rx): (chmux::Sender<String>, chmux::Receiver<String>) =
                            req.accept().await;
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

    fn uds_client() {
        let mut rt = Runtime::new().unwrap();
        rt.block_on(async {
            let socket = UnixStream::connect("/tmp/chmux_test").await.unwrap();

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
                let mut client: chmux::Client<String, _, _> = client;

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
    fn uds_test() {
        env_logger::init();

        println!("Starting server thread...");
        let server_thread = std::thread::spawn(uds_server);
        std::thread::sleep(Duration::from_millis(100));

        println!("String client thread...");
        let client_thread = std::thread::spawn(uds_client);

        println!("Waiting for server thread...");
        server_thread.join().unwrap();
        println!("Waiting for client thread...");
        client_thread.join().unwrap();
    }
}
