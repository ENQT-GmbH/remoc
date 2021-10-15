use remoc::rfn::{CallError, RFn};

use crate::loop_channel;

#[tokio::test]
async fn simple() {
    crate::init();
    let ((mut a_tx, _), (_, mut b_rx)) = loop_channel::<RFn<i16, Result<i16, CallError>>>().await;

    let rfn = RFn::new(|arg: i16| async move { Ok(-arg) });

    println!("Sending remote function");
    a_tx.send(rfn).await.unwrap();
    println!("Receiving remote function");
    let rfn = b_rx.recv().await.unwrap().unwrap();

    println!("calling function");
    let value = 123;
    let result = rfn.call(value).await.unwrap();
    println!("rfn({}) = {}", value, result);
    assert_eq!(result, -value);

    println!("calling function");
    let value = 331;
    let result = rfn.call(value).await.unwrap();
    println!("rfn({}) = {}", value, result);
    assert_eq!(result, -value);
}
