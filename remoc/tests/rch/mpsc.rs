use futures::future;
use rand::Rng;
use remoc::rch::{mpsc, ClosedReason};
use std::time::Duration;
use tokio::time::sleep;

use crate::{droppable_loop_channel, loop_channel};

#[tokio::test]
async fn simple() {
    crate::init();
    let ((mut a_tx, _), (_, mut b_rx)) = loop_channel::<mpsc::Receiver<i16>>().await;

    println!("Sending remote mpsc channel receiver");
    let (tx, rx) = mpsc::channel(16);
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
    assert_eq!(tx.closed_reason(), None);
    rx.close();

    println!("Closing channel");
    tx.closed().await;
    assert!(tx.is_closed());
    assert_eq!(tx.closed_reason(), Some(ClosedReason::Closed));

    println!("Trying send after close");
    match tx.send(0).await {
        Ok(_) => panic!("send succeeded after close"),
        Err(err) if err.is_closed() && err.closed_reason() == Some(ClosedReason::Closed) => (),
        Err(_) => panic!("wrong error after close"),
    }
}

#[tokio::test]
async fn simple_drop() {
    crate::init();
    let ((mut a_tx, _), (_, mut b_rx)) = loop_channel::<mpsc::Receiver<i16>>().await;

    println!("Sending remote mpsc channel receiver");
    let (tx, rx) = mpsc::channel(16);
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
    assert_eq!(tx.closed_reason(), None);
    drop(rx);

    println!("Dropping channel");
    tx.closed().await;
    assert!(tx.is_closed());
    assert_eq!(tx.closed_reason(), Some(ClosedReason::Dropped));

    println!("Trying send after drop");
    match tx.send(0).await {
        Ok(_) => panic!("send succeeded after close"),
        Err(err) if err.is_closed() && err.closed_reason() == Some(ClosedReason::Dropped) => (),
        Err(_) => panic!("wrong error after close"),
    }
}

#[tokio::test]
async fn simple_conn_failure() {
    crate::init();
    let ((mut a_tx, _), (_, mut b_rx), conn) = droppable_loop_channel::<mpsc::Receiver<i16>>().await;

    println!("Sending remote mpsc channel receiver");
    let (tx, rx) = mpsc::channel(16);
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
    assert_eq!(tx.closed_reason(), None);
    drop(conn);

    println!("Dropping connection");
    tx.closed().await;
    assert!(tx.is_closed());
    assert_eq!(tx.closed_reason(), Some(ClosedReason::Failed));

    println!("Trying send after drop");
    match tx.send(0).await {
        Ok(_) => panic!("send succeeded after close"),
        Err(err) if err.is_closed() && err.closed_reason() == Some(ClosedReason::Failed) => (),
        Err(_) => panic!("wrong error after close"),
    }
}

#[tokio::test]
async fn multiple() {
    crate::init();
    let ((mut a_tx, _), (_, mut b_rx)) = loop_channel::<mpsc::Receiver<mpsc::Sender<i16>>>().await;

    println!("Sending remote mpsc channel receiver");
    let (tx, rx) = mpsc::channel(16);
    a_tx.send(rx).await.unwrap();
    println!("Receiving remote mpsc channel receiver");
    let mut rx = b_rx.recv().await.unwrap().unwrap();

    let mut rng = rand::thread_rng();

    let mut tasks = Vec::new();
    for i in 1..1024 {
        println!("Sending sender");
        let (n_tx, mut n_rx) = mpsc::channel(1);
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

#[tokio::test]
async fn forward() {
    crate::init();
    let ((mut a0_tx, _), (_, mut b0_rx)) = loop_channel::<mpsc::Sender<i16>>().await;
    let ((mut a1_tx, _), (_, mut b1_rx)) = loop_channel::<mpsc::Sender<i16>>().await;
    let ((mut a2_tx, _), (_, mut b2_rx)) = loop_channel::<mpsc::Receiver<i16>>().await;
    let ((mut a3_tx, _), (_, mut b3_rx)) = loop_channel::<mpsc::Receiver<i16>>().await;

    let (tx, rx) = mpsc::channel(16);

    println!("Sender 0");
    a0_tx.send(tx).await.unwrap();
    let tx = b0_rx.recv().await.unwrap().unwrap();
    println!("Sender 1");
    a1_tx.send(tx).await.unwrap();
    let tx = b1_rx.recv().await.unwrap().unwrap();
    println!("Sender 0");
    a0_tx.send(tx).await.unwrap();
    let tx = b0_rx.recv().await.unwrap().unwrap();

    println!("Receiver 2");
    a2_tx.send(rx).await.unwrap();
    let rx = b2_rx.recv().await.unwrap().unwrap();
    println!("Receiver 3");
    a3_tx.send(rx).await.unwrap();
    let rx = b3_rx.recv().await.unwrap().unwrap();
    println!("Receiver 3");
    a3_tx.send(rx).await.unwrap();
    let mut rx = b3_rx.recv().await.unwrap().unwrap();

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
    assert_eq!(tx.closed_reason(), None);
    rx.close();

    println!("Closing channel");
    tx.closed().await;
    assert!(tx.is_closed());
    assert_eq!(tx.closed_reason(), Some(ClosedReason::Closed));

    println!("Trying send after close");
    match tx.send(0).await {
        Ok(_) => panic!("send succeeded after close"),
        Err(err) if err.is_closed() && err.closed_reason() == Some(ClosedReason::Closed) => (),
        Err(_) => panic!("wrong error after close"),
    }
}
