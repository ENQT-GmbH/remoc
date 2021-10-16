use crate::loop_channel;

// Avoid imports here to test if proc macro works without imports.

#[derive(serde::Serialize, serde::Deserialize)]
pub enum IncreaseError {
    Overflow,
    Call(remoc::rtc::CallError),
}

impl From<remoc::rtc::CallError> for IncreaseError {
    fn from(err: remoc::rtc::CallError) -> Self {
        Self::Call(err)
    }
}

#[remoc::rtc::remote]
pub trait Counter<CodecA> where CodecA: remoc::codec::Codec {
    async fn value(&self) -> Result<u32, remoc::rtc::CallError>;
    async fn watch(&self) -> Result<remoc::rch::watch::Receiver<u32, CodecA>, remoc::rtc::CallError>;
    #[no_cancel]
    async fn increase(&mut self, #[serde(default)] by: u32) -> Result<(), IncreaseError>;
}

#[tokio::test]
async fn simple() {
    crate::init();
    let ((mut a_tx, _), (_, mut b_rx)) = loop_channel::<RFn<i16, Result<i16, CallError>>>().await;

    println!("Sending remote mpsc channel receiver");
    let (tx, rx) = mpsc::channel(16);
    a_tx.send(rx).await.unwrap();
    println!("Receiving remote mpsc channel receiver");
    let mut rx = b_rx.recv().await.unwrap().unwrap();    
}
