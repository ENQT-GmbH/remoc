use crate::loop_channel;
use remoc::{codec::JsonCodec, robj::lazy::Lazy};

#[tokio::test]
async fn simple() {
    crate::init();
    let ((mut a_tx, _), (_, mut b_rx)) = loop_channel::<Lazy<String, JsonCodec>>().await;

    let value = "test string data".to_string();

    let lazy: Lazy<_, JsonCodec> = Lazy::new(value.clone());

    println!("Sending lazy");
    a_tx.send(lazy).await.unwrap();
    println!("Receiving lazy");
    let lazy = b_rx.recv().await.unwrap().unwrap();

    println!("Fetching lazy");
    println!("reference: {}", *lazy.get().await.unwrap());
    println!("value: {}", lazy.into_inner().await.unwrap());
}
