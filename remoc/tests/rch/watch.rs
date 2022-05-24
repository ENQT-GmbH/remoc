use futures::StreamExt;
use std::time::Duration;
use tokio::time::sleep;

use crate::{droppable_loop_channel, loop_channel};
use remoc::rch::watch::{self, ReceiverStream};

#[tokio::test]
async fn simple() {
    crate::init();
    let ((mut a_tx, _), (_, mut b_rx)) = loop_channel::<watch::Receiver<i16>>().await;

    let start_value = 2;
    let end_value = 124;

    println!("Sending remote mpsc channel receiver");
    let (tx, rx) = watch::channel(start_value);
    a_tx.send(rx).await.unwrap();
    println!("Receiving remote mpsc channel receiver");
    let mut rx = b_rx.recv().await.unwrap().unwrap();

    {
        let value = rx.borrow().unwrap();
        println!("Initial value: {:?}", value);
    }

    let recv_task = tokio::spawn(async move {
        let mut value = *rx.borrow().unwrap();
        assert_eq!(value, start_value);

        while rx.changed().await.is_ok() {
            value = *rx.borrow_and_update().unwrap();
            println!("Received value change: {}", value);
        }

        value = *rx.borrow_and_update().unwrap();
        assert_eq!(value, end_value);
    });

    for value in start_value..=end_value {
        println!("Sending {}", value);
        tx.send(value).unwrap();
        assert_eq!(*tx.borrow(), value);

        if value % 10 == 0 {
            sleep(Duration::from_millis(20)).await;
        }
    }
    drop(tx);

    println!("Waiting for receive task");
    recv_task.await.unwrap();
}

#[tokio::test]
async fn simple_stream() {
    crate::init();
    let ((mut a_tx, _), (_, mut b_rx)) = loop_channel::<watch::Receiver<i16>>().await;

    let start_value = 2;
    let end_value = 124;

    println!("Sending remote mpsc channel receiver");
    let (tx, rx) = watch::channel(start_value);
    a_tx.send(rx).await.unwrap();
    println!("Receiving remote mpsc channel receiver");
    let rx = b_rx.recv().await.unwrap().unwrap();
    let mut rx = ReceiverStream::from(rx);

    let recv_task = tokio::spawn(async move {
        let mut value = 0;
        while let Some(rxed_value) = rx.next().await {
            value = rxed_value.unwrap();
            println!("Received value change: {}", value);
        }

        assert_eq!(value, end_value);
    });

    let mut prev_value = start_value;
    for value in start_value..=end_value {
        println!("Sending {}", value);
        let last_value = tx.send_replace(value);
        assert_eq!(last_value, prev_value);
        assert_eq!(*tx.borrow(), value);
        prev_value = value;

        if value % 10 == 0 {
            sleep(Duration::from_millis(20)).await;

            println!("Modifying");
            tx.send_modify(|v| *v -= 1);
            prev_value -= 1;
        }
    }
    drop(tx);

    println!("Waiting for receive task");
    recv_task.await.unwrap();
}

#[tokio::test]
async fn close() {
    crate::init();
    let ((mut a_tx, _), (_, mut b_rx)) = loop_channel::<watch::Sender<i16>>().await;

    println!("Sending remote mpsc channel sender");
    let (tx, rx) = watch::channel(123);
    a_tx.send(tx).await.unwrap();
    println!("Receiving remote mpsc channel sender");
    let tx = b_rx.recv().await.unwrap().unwrap();

    println!("Cloning receiver");
    let rx2 = rx.clone();

    assert!(!tx.is_closed());

    println!("Dropping first receiver");
    drop(rx);
    assert!(!tx.is_closed());

    println!("Dropping second receiver");
    drop(rx2);

    println!("Waiting for close notification");
    tx.closed().await;
    assert!(tx.is_closed());

    println!("Attempting to send");
    match tx.send(15) {
        Ok(()) => panic!("send succeeded after close"),
        Err(err) if err.is_closed() => (),
        Err(err) => panic!("wrong error after close: {}", err),
    }
}

#[tokio::test]
async fn conn_failure() {
    crate::init();
    let ((mut a_tx, _), (_, mut b_rx), conn) = droppable_loop_channel::<watch::Sender<i16>>().await;

    println!("Sending remote mpsc channel sender");
    let (tx, rx) = watch::channel(123);
    a_tx.send(tx).await.unwrap();
    println!("Receiving remote mpsc channel sender");
    let tx = b_rx.recv().await.unwrap().unwrap();

    println!("Cloning receiver");
    let _rx2 = rx.clone();

    assert!(!tx.is_closed());

    println!("Dropping connection");
    drop(conn);

    println!("Waiting for close notification");
    tx.closed().await;
    assert!(tx.is_closed());

    println!("Attempting to send");
    match tx.send(15) {
        Ok(()) => panic!("send succeeded after close"),
        Err(err) if err.is_closed() => (),
        Err(err) => panic!("wrong error after close: {}", err),
    }
}
