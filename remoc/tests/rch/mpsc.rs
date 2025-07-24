use futures::{future, StreamExt};
use rand::Rng;
use std::time::Duration;

#[cfg(feature = "js")]
use wasm_bindgen_test::wasm_bindgen_test;

use crate::{droppable_loop_channel, loop_channel};
use remoc::{
    codec, exec,
    exec::time::sleep,
    rch::{base, base::SendErrorKind, mpsc, mpsc::SendError, ClosedReason, SendResultExt, SendingError},
};

#[cfg_attr(not(feature = "js"), tokio::test)]
#[cfg_attr(feature = "js", wasm_bindgen_test)]
async fn simple() {
    crate::init();
    let ((mut a_tx, _), (_, mut b_rx)) = loop_channel::<mpsc::Receiver<i16>>().await;

    println!("Sending remote mpsc channel receiver");
    let (tx, rx) = mpsc::channel(16);
    a_tx.send(rx).await.unwrap();
    println!("Receiving remote mpsc channel receiver");
    let mut rx = b_rx.recv().await.unwrap().unwrap();

    for i in 1..1024 {
        println!("Sending {i}");
        let tx = tx.clone();
        tx.send(i).await.unwrap();
        let r = rx.recv().await.unwrap().unwrap();
        println!("Received {r}");
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
        Err(err)
            if err.is_closed() && err.is_disconnected() && err.closed_reason() == Some(ClosedReason::Closed) => {}
        Err(_) => panic!("wrong error after close"),
    }
}

#[cfg_attr(not(feature = "js"), tokio::test)]
#[cfg_attr(feature = "js", wasm_bindgen_test)]
async fn simple_stream() {
    crate::init();
    let ((mut a_tx, _), (_, mut b_rx)) = loop_channel::<mpsc::Receiver<i16>>().await;

    println!("Sending remote mpsc channel receiver");
    let (tx, rx) = mpsc::channel(16);
    a_tx.send(rx).await.unwrap();
    println!("Receiving remote mpsc channel receiver");
    let mut rx = b_rx.recv().await.unwrap().unwrap();

    for i in 1..1024 {
        println!("Sending {i}");
        let tx = tx.clone();
        tx.send(i).await.unwrap();
        let r = rx.next().await.unwrap().unwrap();
        println!("Received {r}");
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
        Err(err)
            if err.is_closed() && err.is_disconnected() && err.closed_reason() == Some(ClosedReason::Closed) => {}
        Err(_) => panic!("wrong error after close"),
    }
}

#[cfg_attr(not(feature = "js"), tokio::test)]
#[cfg_attr(feature = "js", wasm_bindgen_test)]
async fn recv_many() {
    const CAP: usize = 50;

    crate::init();
    let ((mut a_tx, _), (_, mut b_rx)) = loop_channel::<mpsc::Receiver<i16, codec::Default, CAP>>().await;

    println!("Sending remote mpsc channel receiver");
    let (tx, rx) = mpsc::channel(16);
    let rx = rx.set_buffer();
    a_tx.send(rx).await.unwrap();
    println!("Receiving remote mpsc channel receiver");
    let mut rx = b_rx.recv().await.unwrap().unwrap();

    let rng = 1..1024;
    let rng2 = rng.clone();
    tokio::spawn(async move {
        for i in rng2 {
            println!("Sending {i}");
            let tx = tx.clone();
            tx.send(i).await.unwrap();
        }
    });

    let mut i = rng.start;
    let mut max_batch_size = 0;
    loop {
        let mut buf = Vec::with_capacity(CAP);
        let n = rx.recv_many(&mut buf, CAP).await.unwrap();

        println!("Received batch of size {n} / {CAP}");
        if n == 0 {
            break;
        }
        assert_eq!(n, buf.len());
        assert!(n <= CAP);

        for r in buf {
            println!("Received {r}");
            assert_eq!(i, r);
            i += 1;
        }

        if n < 25 {
            sleep(Duration::from_millis(100)).await;
        }
        max_batch_size = max_batch_size.max(n);
    }
    assert_eq!(i, rng.end);
    assert!(max_batch_size >= 10);
}

#[cfg_attr(not(feature = "js"), tokio::test)]
#[cfg_attr(feature = "js", wasm_bindgen_test)]
async fn recv_len() {
    const CAP: usize = 50;

    crate::init();
    let ((mut a_tx, _), (_, mut b_rx)) = loop_channel::<mpsc::Receiver<i16, codec::Default, CAP>>().await;

    println!("Sending remote mpsc channel receiver");
    let (tx, rx) = mpsc::channel(16);
    let rx = rx.set_buffer();
    a_tx.send(rx).await.unwrap();
    println!("Receiving remote mpsc channel receiver");
    let mut rx = b_rx.recv().await.unwrap().unwrap();

    assert_eq!(rx.len(), 0);
    assert!(rx.is_empty());

    let rng = 1..1024;
    let rng2 = rng.clone();
    tokio::spawn(async move {
        for i in rng2 {
            println!("Sending {i}");
            let tx = tx.clone();
            tx.send(i).await.unwrap();
        }
    });

    while rx.len() < CAP {
        sleep(Duration::from_millis(100)).await;
    }

    assert!(!rx.is_empty());

    for i in 0..CAP {
        assert_eq!(rx.len(), CAP - i);
        rx.recv().await.unwrap();
    }

    assert_eq!(rx.len(), 0);
    assert!(rx.is_empty());
}

#[cfg_attr(not(feature = "js"), tokio::test)]
#[cfg_attr(feature = "js", wasm_bindgen_test)]
async fn simple_close() {
    crate::init();
    let ((mut a_tx, _), (_, mut b_rx)) = loop_channel::<mpsc::Receiver<i16>>().await;

    println!("Sending remote mpsc channel receiver");
    let (tx, rx) = mpsc::channel(16);
    a_tx.send(rx).await.unwrap();
    println!("Receiving remote mpsc channel receiver");
    let mut rx = b_rx.recv().await.unwrap().unwrap();

    for i in 1..1024 {
        println!("Sending {i}");
        let tx = tx.clone();

        if tx.send(i).await.into_closed().unwrap() {
            println!("Receiver was closed");
            break;
        }

        let r = rx.recv().await.unwrap().unwrap();
        println!("Received {r}");
        assert_eq!(i, r, "send/receive mismatch");

        if r == 512 {
            println!("Closing receiver");
            rx.close();
        }
    }

    println!("Verifying that channel is closed");
    assert!(tx.is_closed());
    assert_eq!(tx.closed_reason(), Some(ClosedReason::Closed));

    println!("Trying send after close");
    match tx.send(0).await {
        Ok(_) => panic!("send succeeded after close"),
        Err(err)
            if err.is_closed() && err.is_disconnected() && err.closed_reason() == Some(ClosedReason::Closed) => {}
        Err(_) => panic!("wrong error after close"),
    }
}

#[cfg_attr(not(feature = "js"), tokio::test)]
#[cfg_attr(feature = "js", wasm_bindgen_test)]
async fn simple_drop() {
    crate::init();
    let ((mut a_tx, _), (_, mut b_rx)) = loop_channel::<mpsc::Receiver<i16>>().await;

    println!("Sending remote mpsc channel receiver");
    let (tx, rx) = mpsc::channel(16);
    a_tx.send(rx).await.unwrap();
    println!("Receiving remote mpsc channel receiver");
    let mut rx = b_rx.recv().await.unwrap().unwrap();

    for i in 1..1024 {
        println!("Sending {i}");
        let tx = tx.clone();
        tx.send(i).await.unwrap();
        let r = rx.recv().await.unwrap().unwrap();
        println!("Received {r}");
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
        Err(err)
            if err.is_disconnected()
                && !err.is_closed()
                && err.closed_reason() == Some(ClosedReason::Dropped) => {}
        Err(_) => panic!("wrong error after close"),
    }
}

#[cfg_attr(not(feature = "js"), tokio::test)]
#[cfg_attr(feature = "js", wasm_bindgen_test)]
async fn simple_conn_failure() {
    crate::init();
    let ((mut a_tx, _), (_, mut b_rx), conn) = droppable_loop_channel::<mpsc::Receiver<i16>>().await;

    println!("Sending remote mpsc channel receiver");
    let (tx, rx) = mpsc::channel(16);
    a_tx.send(rx).await.unwrap();
    println!("Receiving remote mpsc channel receiver");
    let mut rx = b_rx.recv().await.unwrap().unwrap();

    for i in 1..1024 {
        println!("Sending {i}");
        let tx = tx.clone();
        tx.send(i).await.unwrap();
        let r = rx.recv().await.unwrap().unwrap();
        println!("Received {r}");
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
        Err(err)
            if err.is_disconnected() && !err.is_closed() && err.closed_reason() == Some(ClosedReason::Failed) => {
        }
        Err(_) => panic!("wrong error after close"),
    }
}

#[cfg_attr(not(feature = "js"), tokio::test)]
#[cfg_attr(feature = "js", wasm_bindgen_test)]
async fn two_sender_conn_failure() {
    crate::init();
    let ((mut a_tx, _), (_, mut b_rx), conn1) = droppable_loop_channel::<mpsc::Sender<i16>>().await;
    let ((mut c_tx, _), (_, mut d_rx), _conn2) = droppable_loop_channel::<mpsc::Sender<i16>>().await;

    let mut conn1 = Some(conn1);

    println!("Sending two remote mpsc channel senders");
    let (tx, mut rx) = mpsc::channel(16);
    a_tx.send(tx.clone()).await.unwrap();
    c_tx.send(tx.clone()).await.unwrap();

    println!("Receiving two remote mpsc channel receivers");
    let tx1 = b_rx.recv().await.unwrap().unwrap();
    let tx2 = d_rx.recv().await.unwrap().unwrap();

    for i in 1..100 {
        println!("Sending {i} over connection 1");
        match tx1.send(i).await {
            Ok(sent) => println!("Send ok: {sent:?}"),
            Err(err) => {
                if conn1.is_some() {
                    panic!("Send failed before connection drop");
                }
                println!("Send failed: {} with reason {:?}", &err, err.closed_reason());
                assert_eq!(err.closed_reason(), Some(ClosedReason::Failed));
            }
        }

        println!("Sending {i} over connection 2");
        tx2.send(i).await.unwrap();

        if i == 50 {
            println!("Dropping connection 1");
            conn1 = None;
        }

        if conn1.is_some() {
            let r = rx.recv().await.unwrap().unwrap();
            println!("Received {r} first time");
            assert_eq!(i, r, "send/receive mismatch");

            let r = rx.recv().await.unwrap().unwrap();
            println!("Received {r} second time");
            assert_eq!(i, r, "send/receive mismatch");
        } else {
            let r = rx.recv().await.unwrap().unwrap();
            println!("Received {r} first time");

            if let Ok(r) = rx.try_recv() {
                println!("Received {r} second time");
            }
        }
    }

    println!("Waiting for sender 1 to be closed");
    tx1.closed().await;
    assert!(tx1.is_closed());
    assert_eq!(tx1.closed_reason(), Some(ClosedReason::Failed));

    println!("Trying send after connection failure over sender 1");
    match tx1.send(0).await {
        Ok(_) => panic!("send succeeded after close"),
        Err(err) if err.is_disconnected() && err.closed_reason() == Some(ClosedReason::Failed) => (),
        Err(_) => panic!("wrong error after close"),
    }

    println!("Verifying that channel is open");
    assert!(!tx.is_closed());
    assert_eq!(tx.closed_reason(), None);

    println!("Closing receiver");
    rx.close();

    println!("Waiting for sender 2 to be closed");
    tx2.closed().await;
    assert!(tx2.is_closed());
    assert_eq!(tx2.closed_reason(), Some(ClosedReason::Closed));

    println!("Trying send after close over sender 2");
    match tx2.send(0).await {
        Ok(_) => panic!("send succeeded after close"),
        Err(err) if err.is_disconnected() && err.closed_reason() == Some(ClosedReason::Closed) => (),
        Err(_) => panic!("wrong error after close"),
    }
}

#[cfg_attr(not(feature = "js"), tokio::test)]
#[cfg_attr(feature = "js", wasm_bindgen_test)]
async fn multiple() {
    crate::init();
    let ((mut a_tx, _), (_, mut b_rx)) = loop_channel::<mpsc::Receiver<mpsc::Sender<i16>>>().await;

    println!("Sending remote mpsc channel receiver");
    let (tx, rx) = mpsc::channel(16);
    a_tx.send(rx).await.unwrap();
    println!("Receiving remote mpsc channel receiver");
    let mut rx = b_rx.recv().await.unwrap().unwrap();

    let mut rng = rand::rng();

    let mut tasks = Vec::new();
    for i in 1..1024 {
        println!("Sending sender");
        let (n_tx, mut n_rx) = mpsc::channel(1);
        tx.send(n_tx).await.unwrap();
        println!("Receiving sender");
        let n_tx = rx.recv().await.unwrap().unwrap();

        let dur = Duration::from_millis(rng.random_range(0..100));
        let task = exec::spawn(async move {
            sleep(dur).await;

            println!("Sending {i}");
            n_tx.send(i).await.unwrap();
            let r = n_rx.recv().await.unwrap().unwrap();
            println!("Received {r}");
            assert_eq!(i, r, "send/receive mismatch");

            drop(n_tx);
            assert!(n_rx.recv().await.unwrap().is_none());
        });
        tasks.push(task);
    }

    future::try_join_all(tasks).await.unwrap();
}

#[cfg_attr(not(feature = "js"), tokio::test)]
#[cfg_attr(feature = "js", wasm_bindgen_test)]
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
        println!("Sending {i}");
        let tx = tx.clone();
        tx.send(i).await.unwrap();
        let r = rx.recv().await.unwrap().unwrap();
        println!("Received {r}");
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

#[cfg_attr(not(feature = "js"), tokio::test)]
#[cfg_attr(feature = "js", wasm_bindgen_test)]
async fn max_item_size_exceeded() {
    crate::init();
    if !remoc::exec::are_threads_available().await {
        println!("test requires threads");
        return;
    }

    let ((mut a_tx, _), (_, mut b_rx)) = loop_channel::<mpsc::Receiver<Vec<u8>>>().await;

    println!("Sending remote mpsc channel receiver");
    let (tx, rx) = mpsc::channel(16);
    a_tx.send(rx).await.unwrap();
    println!("Receiving remote mpsc channel receiver");
    let mut rx = b_rx.recv().await.unwrap().unwrap();

    assert_eq!(tx.max_item_size(), rx.max_item_size());
    let max_item_size = tx.max_item_size();
    println!("Maximum send and recv item size is {max_item_size}");

    // Happy case: sent data size is under limit.
    // JSON encoding will result in much larger transfer size.
    let elems = max_item_size / 10;
    let data = vec![127; elems];
    println!("Sending {elems} elements, which is under limit");
    let mut sendings = Vec::new();
    sendings.push(tx.send(data.clone()).await.unwrap());
    println!("Receiving...");
    let rxed = rx.recv().await.unwrap().unwrap();
    println!("Received {} elements", rxed.len());
    assert_eq!(data, rxed, "send/receive mismatch");
    println!();

    for sending in sendings {
        sending.await.unwrap();
    }

    // Failure case: sent data size exceeds limits.
    let elems = max_item_size * 10;
    let data = vec![127; elems];
    println!("Sending {elems} elements, which is over limit");
    let failed_sending = tx.send(data.clone()).await.unwrap();

    println!("Receiving...");
    let rxed = rx.recv().await;
    println!("Receive result: {rxed:?}");
    assert!(matches!(rxed, Ok(None)));
    assert!(matches!(
        failed_sending.await,
        Err(SendingError::Send(base::SendError { kind: SendErrorKind::MaxItemSizeExceeded, .. }))
    ));

    // Send one more element to obtain error.
    println!("Sending 1 element");
    let res = tx.send(vec![1]).await;
    println!("Send result: {res:?}");
    assert!(matches!(res, Err(SendError::RemoteSend(SendErrorKind::MaxItemSizeExceeded))));
    println!();

    println!("Verifying that sender is closed");
    assert!(tx.is_closed());
    assert_eq!(tx.closed_reason(), Some(ClosedReason::Failed));
    println!("Close reason: {:?}", tx.closed_reason());
}
