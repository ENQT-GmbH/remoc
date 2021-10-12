use remoc::{codec::JsonCodec, rsync::mpsc};

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
