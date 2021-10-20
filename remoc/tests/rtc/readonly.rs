use futures::join;

use crate::loop_channel;

// Avoid imports here to test if proc macro works without imports.

#[derive(Debug, serde::Serialize, serde::Deserialize)]
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
pub trait ReadValue {
    async fn value(&self) -> Result<u32, remoc::rtc::CallError>;
}

pub struct ReadValueObj {
    value: u32,
}

impl ReadValueObj {
    pub fn new(value: u32) -> Self {
        Self { value }
    }
}

#[remoc::rtc::async_trait]
impl ReadValue for ReadValueObj {
    async fn value(&self) -> Result<u32, remoc::rtc::CallError> {
        Ok(self.value)
    }
}

#[tokio::test]
async fn simple() {
    use remoc::rtc::ServerRef;

    crate::init();
    let ((mut a_tx, _), (_, mut b_rx)) = loop_channel::<ReadValueClient>().await;

    println!("Creating server");
    let obj = ReadValueObj::new(123);
    let (server, client) = ReadValueServerRef::new(&obj, 1);

    println!("Sending client");
    a_tx.send(client).await.unwrap();

    let client_task = async move {
        println!("Receiving client");
        let client = b_rx.recv().await.unwrap().unwrap();

        println!("value: {}", client.value().await.unwrap());
        assert_eq!(client.value().await.unwrap(), 123);
    };

    join!(client_task, server.serve());
}
