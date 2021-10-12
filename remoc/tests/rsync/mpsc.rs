use futures::future::{self};
use rand::Rng;
use remoc::{codec::JsonCodec, rsync::mpsc};
use std::time::Duration;
use tokio::time::sleep;

use crate::loop_channel;

#[tokio::test]
async fn simple() {
    crate::init();
    let ((mut a_tx, _), (_, mut b_rx)) = loop_channel::<mpsc::Receiver<i16, JsonCodec, 16>>().await;

    println!("Sending remote mpsc channel receiver");
    let (tx, rx) = mpsc::channel::<_, _, 16, 16>(16);
    a_tx.send(rx).await.unwrap();
    println!("Receiving remote mpsc channel receiver");
    let mut rx = b_rx.recv().await.unwrap().unwrap();

    for i in 1..1024 {
        println!("Sending {}", i);
        let tx = tx.clone();
        tx.send(i).await.unwrap();
        let r = rx.recv().await.unwrap().unwrap();
        println!("Received {}", r);
        assert_eq!(i, r, "send/receive mismatch");
    }

    println!("Verifying that channel is open");
    assert!(!tx.is_closed());
    rx.close();

    println!("Closing channel");
    tx.closed().await;
    assert!(tx.is_closed());

    println!("Trying send after close");
    match tx.send(0).await {
        Ok(_) => panic!("send succeeded after close"),
        Err(err) if err.is_closed() => (),
        Err(_) => panic!("wrong error after close"),
    }
}

#[tokio::test]
async fn multiple() {
    crate::init();
    let ((mut a_tx, _), (_, mut b_rx)) =
        loop_channel::<mpsc::Receiver<mpsc::Sender<i16, JsonCodec, 1>, JsonCodec, 16>>().await;

    println!("Sending remote mpsc channel receiver");
    let (tx, rx) = mpsc::channel::<_, _, 16, 16>(16);
    a_tx.send(rx).await.unwrap();
    println!("Receiving remote mpsc channel receiver");
    let mut rx = b_rx.recv().await.unwrap().unwrap();

    let mut rng = rand::thread_rng();

    let mut tasks = Vec::new();
    for i in 1..1024 {
        println!("Sending sender");
        let (n_tx, mut n_rx) = mpsc::channel::<_, _, 1, 1>(1);
        tx.send(n_tx).await.unwrap();
        println!("Receiving sender");
        let n_tx = rx.recv().await.unwrap().unwrap();

        let dur = Duration::from_millis(rng.gen_range(0..100));
        let task = tokio::spawn(async move {
            sleep(dur).await;

            println!("Sending {}", i);
            n_tx.send(i).await.unwrap();
            let r = n_rx.recv().await.unwrap().unwrap();
            println!("Received {}", r);
            assert_eq!(i, r, "send/receive mismatch");

            drop(n_tx);
            assert!(n_rx.recv().await.unwrap().is_none());
        });
        tasks.push(task);
    }

    future::try_join_all(tasks).await.unwrap();
}
