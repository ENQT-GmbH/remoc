use std::time::Duration;

use remoc::rch::watch;
use tokio::time::sleep;

use crate::loop_channel;

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
        assert_eq!(*tx.borrow().unwrap(), value);

        if value % 10 == 0 {
            sleep(Duration::from_millis(20)).await;
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
