use bytes::{Buf, Bytes};
use futures::join;
use rand::{Rng, RngCore};
use remoc::{chmux::Received, rch::bin};

use crate::loop_channel;

#[tokio::test]
async fn loopback() {
    crate::init();
    let ((mut a_tx, _), (_, mut b_rx)) = loop_channel::<(bin::Sender, bin::Receiver)>().await;

    println!("Sending remote bin channel sender and receiver");
    let (tx1, rx1) = bin::channel();
    let (tx2, rx2) = bin::channel();
    a_tx.send((tx1, rx2)).await.unwrap();
    println!("Receiving remote bin channel sender and receiver");
    let (tx1, rx2) = b_rx.recv().await.unwrap().unwrap();

    let reply_task = tokio::spawn(async move {
        let mut rx1 = rx1.into_inner().await.unwrap();
        let mut tx2 = tx2.into_inner().await.unwrap();

        loop {
            match rx1.recv_any().await.unwrap() {
                Some(Received::Data(data)) => {
                    println!("Echoing data of length {}", data.remaining());
                    tx2.send(data.into()).await.unwrap();
                }
                Some(Received::Chunks) => {
                    println!("Echoing big data stream");
                    let mut i = 0;
                    let mut cs = tx2.send_chunks();
                    while let Some(chunk) = rx1.recv_chunk().await.unwrap() {
                        println!("Echoing chunk {} of size {}", i, chunk.len());
                        cs = cs.send(chunk).await.unwrap();
                        i += 1;
                    }
                    cs.finish().await.unwrap();
                }
                Some(_) => (),
                None => break,
            }
        }
    });

    let mut tx1 = tx1.into_inner().await.unwrap();
    let mut rx2 = rx2.into_inner().await.unwrap();

    rx2.set_max_data_size(1_000_000);

    let mut rng = rand::thread_rng();
    for i in 1..100 {
        let size = if i % 2 == 0 { rng.gen_range(0..1_000_000) } else { 1024 };
        let mut data: Vec<u8> = Vec::new();
        data.resize(size, 0);
        rng.fill_bytes(&mut data);
        let data = Bytes::from(data);

        println!("Sending message of length {}", data.len());
        let (send, recv) = join!(tx1.send(data.clone()), rx2.recv());
        send.unwrap();
        let data_recv = recv.unwrap().unwrap();
        println!("Received reply of length {}", data_recv.remaining());
        let data_recv = Bytes::from(data_recv);
        assert_eq!(data, data_recv, "mismatched echo reply");
    }
    drop(tx1);

    if rx2.recv().await.unwrap().is_some() {
        panic!("received data after close");
    }

    reply_task.await.unwrap();
}
