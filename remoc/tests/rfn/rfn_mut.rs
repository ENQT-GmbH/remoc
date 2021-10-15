use remoc::rfn::{CallError, RFnMut};

use crate::loop_channel;

#[tokio::test]
async fn simple() {
    crate::init();
    let ((mut a_tx, _), (_, mut b_rx)) = loop_channel::<RFnMut<i16, Result<i16, CallError>>>().await;

    let mut counter = 0;
    let rfn = RFnMut::new(move |arg: i16| {
        counter += arg;
        async move { Ok(counter) }
    });

    println!("Sending remote function");
    a_tx.send(rfn).await.unwrap();
    println!("Receiving remote function");
    let mut rfn = b_rx.recv().await.unwrap().unwrap();

    println!("calling function");
    let result = rfn.call(12).await.unwrap();
    println!("rfn(12) = {}", result);
    assert_eq!(result, 12);

    println!("calling function");
    let result = rfn.call(33).await.unwrap();
    println!("rfn(33) = {}", result);
    assert_eq!(result, 45);
}
