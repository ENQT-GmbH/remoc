use futures::StreamExt;
use std::time::Duration;

#[cfg(feature = "js")]
use wasm_bindgen_test::wasm_bindgen_test;

use crate::{droppable_loop_channel, loop_channel};
use remoc::{
    executor,
    executor::time::sleep,
    rch::{
        base::SendErrorKind,
        watch::{self, ChangedError, ReceiverStream, SendError},
    },
};

#[cfg_attr(not(feature = "js"), tokio::test)]
#[cfg_attr(feature = "js", wasm_bindgen_test)]
async fn simple() {
    crate::init();
    let ((mut a_tx, _), (_, mut b_rx)) = loop_channel::<watch::Receiver<i16>>().await;

    let start_value = 2;
    let end_value = 124;

    println!("Sending remote mpsc channel receiver");
    let (mut tx, rx) = watch::channel(start_value);
    a_tx.send(rx).await.unwrap();
    println!("Receiving remote mpsc channel receiver");
    let mut rx = b_rx.recv().await.unwrap().unwrap();

    {
        let value = rx.borrow().unwrap();
        println!("Initial value: {value:?}");
    }

    let recv_task = executor::spawn(async move {
        let mut value = *rx.borrow().unwrap();
        assert_eq!(value, start_value);

        while rx.changed().await.is_ok() {
            value = *rx.borrow_and_update().unwrap();
            println!("Received value change: {value}");
        }

        value = *rx.borrow_and_update().unwrap();
        assert_eq!(value, end_value);
    });

    for value in start_value..=end_value {
        println!("Sending {value}");
        tx.send(value).unwrap();
        assert_eq!(*tx.borrow(), value);

        if value % 10 == 0 {
            sleep(Duration::from_millis(20)).await;
        }
    }

    tx.check().unwrap();
    drop(tx);

    println!("Waiting for receive task");
    recv_task.await.unwrap();
}

#[cfg_attr(not(feature = "js"), tokio::test)]
#[cfg_attr(feature = "js", wasm_bindgen_test)]
async fn simple_stream() {
    crate::init();
    let ((mut a_tx, _), (_, mut b_rx)) = loop_channel::<watch::Receiver<i16>>().await;

    let start_value = 2;
    let end_value = 124;

    println!("Sending remote mpsc channel receiver");
    let (mut tx, rx) = watch::channel(start_value);
    a_tx.send(rx).await.unwrap();
    println!("Receiving remote mpsc channel receiver");
    let rx = b_rx.recv().await.unwrap().unwrap();
    let mut rx = ReceiverStream::from(rx);

    let recv_task = executor::spawn(async move {
        let mut value = 0;
        while let Some(rxed_value) = rx.next().await {
            value = rxed_value.unwrap();
            println!("Received value change: {value}");
        }

        assert_eq!(value, end_value);
    });

    let mut prev_value = start_value;
    for value in start_value..=end_value {
        println!("Sending {value}");
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

    tx.check().unwrap();
    drop(tx);

    println!("Waiting for receive task");
    recv_task.await.unwrap();
}

#[cfg_attr(not(feature = "js"), tokio::test)]
#[cfg_attr(feature = "js", wasm_bindgen_test)]
async fn modify_stream() {
    crate::init();
    let ((mut a_tx, _), (_, mut b_rx)) = loop_channel::<watch::Receiver<i16>>().await;

    let start_value = 2;
    let end_value = 124;

    println!("Sending remote mpsc channel receiver");
    let (mut tx, rx) = watch::channel(start_value);
    a_tx.send(rx).await.unwrap();
    println!("Receiving remote mpsc channel receiver");
    let rx = b_rx.recv().await.unwrap().unwrap();
    let mut rx = ReceiverStream::from(rx);

    let recv_task = executor::spawn(async move {
        let mut value = 0;
        while let Some(rxed_value) = rx.next().await {
            value = rxed_value.unwrap();
            println!("Received value change: {value}");
        }

        assert_eq!(value, end_value);
    });

    for value in (start_value + 1)..=end_value {
        println!("Modifying {value}");
        tx.send_modify(|v| *v += 1);

        if value % 10 == 0 {
            sleep(Duration::from_millis(20)).await;
        }
    }

    tx.check().unwrap();
    drop(tx);

    println!("Waiting for receive task");
    recv_task.await.unwrap();
}

#[cfg_attr(not(feature = "js"), tokio::test)]
#[cfg_attr(feature = "js", wasm_bindgen_test)]
async fn close() {
    crate::init();
    let ((mut a_tx, _), (_, mut b_rx)) = loop_channel::<watch::Sender<i16>>().await;

    println!("Sending remote mpsc channel sender");
    let (tx, rx) = watch::channel(123);
    a_tx.send(tx).await.unwrap();
    println!("Receiving remote mpsc channel sender");
    let mut tx = b_rx.recv().await.unwrap().unwrap();

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
    tx.check().unwrap();

    println!("Attempting to send");
    match tx.send(15) {
        Ok(()) => panic!("send succeeded after close"),
        Err(err) if err.is_closed() => (),
        Err(err) => panic!("wrong error after close: {err}"),
    }
}

#[cfg_attr(not(feature = "js"), tokio::test)]
#[cfg_attr(feature = "js", wasm_bindgen_test)]
async fn conn_failure() {
    crate::init();
    let ((mut a_tx, _), (_, mut b_rx), conn) = droppable_loop_channel::<watch::Sender<i16>>().await;

    println!("Sending remote mpsc channel sender");
    let (tx, rx) = watch::channel(123);
    a_tx.send(tx).await.unwrap();
    println!("Receiving remote mpsc channel sender");
    let mut tx = b_rx.recv().await.unwrap().unwrap();

    println!("Cloning receiver");
    let _rx2 = rx.clone();

    assert!(!tx.is_closed());

    println!("Dropping connection");
    drop(conn);

    println!("Waiting for close notification");
    tx.closed().await;
    assert!(tx.is_closed());
    tx.check().unwrap();

    println!("Attempting to send");
    match tx.send(15) {
        Ok(()) => panic!("send succeeded after close"),
        Err(err) if err.is_closed() => (),
        Err(err) => panic!("wrong error after close: {err}"),
    }
}

#[cfg_attr(not(feature = "js"), tokio::test)]
#[cfg_attr(feature = "js", wasm_bindgen_test)]
async fn max_item_size_exceeded() {
    crate::init();
    if !remoc::executor::are_threads_available() {
        println!("test requires threads");
        return;
    }

    let ((mut a_tx, _), (_, mut b_rx)) = loop_channel::<watch::Receiver<Vec<u8>>>().await;

    println!("Sending remote mpsc channel receiver");
    let (mut tx, rx) = watch::channel(Vec::new());
    a_tx.send(rx).await.unwrap();
    println!("Receiving remote mpsc channel receiver");
    let mut rx = b_rx.recv().await.unwrap().unwrap();

    assert_eq!(tx.max_item_size(), rx.max_item_size());
    let max_item_size = tx.max_item_size();
    println!("Maximum send and recv item size is {max_item_size}");

    {
        let value = rx.borrow().unwrap();
        println!("Initial value: {value:?}");
    }

    let recv_task = executor::spawn(async move {
        loop {
            let res = rx.changed().await;
            println!("RX changed result: {res:?}");
            if res.is_err() {
                break res;
            }

            let value = rx.borrow_and_update().unwrap().clone();
            println!("Received value change: {} elements", value.len());
        }
    });

    // Happy case: sent data size is under limit.
    // JSON encoding will result in much larger transfer size.
    let elems = max_item_size / 10;
    println!("Sending {elems} elements");
    let value = vec![100; elems];
    tx.send(value.clone()).unwrap();
    assert_eq!(*tx.borrow(), value);

    sleep(Duration::from_millis(100)).await;

    // Failure case: sent data size exceeds limits.
    let elems = max_item_size * 10;
    println!("Sending {elems} elements");
    let value = vec![100; elems];
    tx.send(value.clone()).unwrap();
    assert_eq!(*tx.borrow(), value);

    println!("Waiting for receive task");
    assert!(matches!(recv_task.await.unwrap(), Err(ChangedError::Closed)));

    // Send one more element to obtain error.
    println!("Sending one more element to obtain error");
    let res = tx.send(vec![1; 1]);
    println!("Result: {res:?}");
    assert!(matches!(res, Err(SendError::RemoteSend(SendErrorKind::MaxItemSizeExceeded))));

    // Test error clearing.
    assert!(matches!(tx.error(), Some(SendError::RemoteSend(SendErrorKind::MaxItemSizeExceeded))));
    tx.clear_error();
    assert!(tx.error().is_none());
    tx.check().unwrap();
}

#[cfg_attr(not(feature = "js"), tokio::test)]
#[cfg_attr(feature = "js", wasm_bindgen_test)]
async fn max_item_size_exceeded_check() {
    crate::init();
    if !remoc::executor::are_threads_available() {
        println!("test requires threads");
        return;
    }

    let ((mut a_tx, _), (_, mut b_rx)) = loop_channel::<watch::Receiver<Vec<u8>>>().await;

    println!("Sending remote mpsc channel receiver");
    let (mut tx, rx) = watch::channel(Vec::new());
    a_tx.send(rx).await.unwrap();
    println!("Receiving remote mpsc channel receiver");
    let mut rx = b_rx.recv().await.unwrap().unwrap();

    assert_eq!(tx.max_item_size(), rx.max_item_size());
    let max_item_size = tx.max_item_size();
    println!("Maximum send and recv item size is {max_item_size}");

    {
        let value = rx.borrow().unwrap();
        println!("Initial value: {value:?}");
    }

    let recv_task = executor::spawn(async move {
        loop {
            let res = rx.changed().await;
            println!("RX changed result: {res:?}");
            if res.is_err() {
                break res;
            }

            let value = rx.borrow_and_update().unwrap().clone();
            println!("Received value change: {} elements", value.len());
        }
    });

    // Happy case: sent data size is under limit.
    // JSON encoding will result in much larger transfer size.
    let elems = max_item_size / 10;
    println!("Sending {elems} elements");
    let value = vec![100; elems];
    tx.send(value.clone()).unwrap();
    assert_eq!(*tx.borrow(), value);

    sleep(Duration::from_millis(100)).await;

    // Failure case: sent data size exceeds limits.
    let elems = max_item_size * 10;
    println!("Sending {elems} elements");
    let value = vec![100; elems];
    tx.send(value.clone()).unwrap();
    assert_eq!(*tx.borrow(), value);

    println!("Wait for sender close");
    tx.closed().await;
    let res = tx.check();
    println!("Sender check result: {res:?}");
    assert!(matches!(res, Err(SendError::RemoteSend(SendErrorKind::MaxItemSizeExceeded))));

    println!("Waiting for receive task");
    assert!(matches!(recv_task.await.unwrap(), Err(ChangedError::Closed)));
}
