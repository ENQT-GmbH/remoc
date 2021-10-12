use futures::try_join;
use remoc::{
    codec::JsonCodec,
    rsync::{broadcast, mpsc},
};
use std::time::Duration;
use tokio::time::sleep;

use crate::loop_channel_with_cfg;

//#[tokio::test(flavor = "multi_thread", worker_threads = 16)]
#[tokio::test]
async fn simple() {
    crate::init();
    let cfg = remoc::chmux::Cfg { chunk_size: 4, receive_buffer: 4, ..Default::default() };
    let ((mut a_tx, _), (_, mut b_rx)) =
        loop_channel_with_cfg::<broadcast::Receiver<(i16, mpsc::Sender<(), JsonCodec, 1>), JsonCodec, 16>>(&cfg)
            .await;

    let (tx, rx1) = broadcast::channel::<_, _, 16>(16);
    let rx2 = tx.subscribe::<16>(16);
    let rx3 = tx.subscribe::<16>(16);

    let send_task = tokio::spawn(async move {
        println!("Sending remote broadcast channel receivers");
        a_tx.send(rx1).await.unwrap();
        a_tx.send(rx2).await.unwrap();
        a_tx.send(rx3).await.unwrap();
    });

    println!("Receiving remote broadcast channel receivers");
    let mut rx1 = b_rx.recv().await.unwrap().unwrap();
    let mut rx2 = b_rx.recv().await.unwrap().unwrap();
    let mut rx3 = b_rx.recv().await.unwrap().unwrap();

    send_task.await.unwrap();

    let rx1_task = tokio::spawn(async move {
        let mut i = 0;
        loop {
            match rx1.recv().await {
                Ok((msg, reply_tx)) => {
                    println!("RX1: {}", msg);
                    assert_eq!(msg, i);
                    reply_tx.send(()).await.unwrap();
                    i += 1;
                }
                Err(err) if err.is_closed() => break,
                Err(err) => panic!("RX1 error: {}", err),
            }
        }
    });

    let rx2_task = tokio::spawn(async move {
        let mut i = 0;
        loop {
            match rx2.recv().await {
                Ok((msg, reply_tx)) => {
                    println!("RX2: {}", msg);
                    assert_eq!(msg, i);
                    reply_tx.send(()).await.unwrap();
                    i += 1;
                }
                Err(err) if err.is_closed() => break,
                Err(err) => panic!("RX2 error: {}", err),
            }
        }
    });

    let rx3_task = tokio::spawn(async move {
        sleep(Duration::from_secs(5)).await;

        let mut lagged = false;
        loop {
            match rx3.recv().await {
                Ok((msg, _reply_tx)) => {
                    println!("RX3: {}", msg);
                }
                Err(err) if err.is_closed() => break,
                Err(err) if err.is_lagged() => {
                    lagged = true;
                    println!("RX3 lagged");
                }
                Err(err) => panic!("RX3 error: {}", err),
            }
        }
        assert!(lagged, "RX3 did not lag behind");
    });

    for i in 0..128 {
        println!("Sending {}", i);
        let (reply_tx, mut reply_rx) = mpsc::channel::<_, _, 1, 1>(1);
        tx.send((i, reply_tx)).unwrap();

        for r in 1..=2 {
            println!("Waiting for reply {}/2", r);
            reply_rx.recv().await.unwrap();
        }
    }
    drop(tx);

    println!("Waiting for tasks to finish");
    try_join!(rx1_task, rx2_task, rx3_task).unwrap();
}
