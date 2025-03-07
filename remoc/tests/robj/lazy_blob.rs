use rand::{Rng, RngCore};

#[cfg(feature = "js")]
use wasm_bindgen_test::wasm_bindgen_test;

use crate::loop_channel;
use remoc::robj::lazy_blob::LazyBlob;

#[cfg_attr(not(feature = "js"), tokio::test)]
#[cfg_attr(feature = "js", wasm_bindgen_test)]
async fn simple() {
    crate::init();
    let ((mut a_tx, _), (_, mut b_rx)) = loop_channel::<LazyBlob>().await;

    let mut rng = rand::rng();
    let size = rng.random_range(10_000_000..15_000_000);
    let mut data = vec![0; size];
    rng.fill_bytes(&mut data);

    println!("Creating lazy blob of size {} bytes", data.len());
    let lazy: LazyBlob = LazyBlob::new(data.clone().into());

    println!("Sending lazy blob");
    a_tx.send(lazy).await.unwrap();
    println!("Receiving lazy blob");
    let lazy = b_rx.recv().await.unwrap().unwrap();

    println!("Length is {} bytes", lazy.len().unwrap());
    assert_eq!(lazy.len().unwrap(), size);

    println!("Fetching reference");
    let fetched = lazy.get().await.unwrap();
    assert_eq!(Vec::from(fetched), data);

    println!("Fetching value");
    let fetched = lazy.into_inner().await.unwrap();
    assert_eq!(Vec::from(fetched), data);
}
