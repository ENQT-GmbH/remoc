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
pub trait Counter {
    async fn value(self) -> Result<u32, remoc::rtc::CallError>;
    async fn value_plus(self, add: u32) -> Result<u32, remoc::rtc::CallError>;
    #[no_cancel]
    async fn increase(&mut self, #[serde(default)] by: u32) -> Result<(), IncreaseError>;
}

pub struct CounterObj {
    value: u32,
    watchers: Vec<remoc::rch::watch::Sender<u32>>,
}

impl CounterObj {
    pub fn new() -> Self {
        Self { value: 0, watchers: Vec::new() }
    }
}

#[remoc::rtc::async_trait]
impl Counter for CounterObj {
    async fn value(self) -> Result<u32, remoc::rtc::CallError> {
        Ok(self.value)
    }

    async fn value_plus(self, add: u32) -> Result<u32, remoc::rtc::CallError> {
        Ok(self.value + add)
    }

    async fn increase(&mut self, by: u32) -> Result<(), IncreaseError> {
        match self.value.checked_add(by) {
            Some(new_value) => self.value = new_value,
            None => return Err(IncreaseError::Overflow),
        }

        for watch in &self.watchers {
            let _ = watch.send(self.value);
        }

        Ok(())
    }
}

#[tokio::test]
async fn simple() {
    use remoc::rtc::Server;

    crate::init();
    let ((mut a_tx, _), (_, mut b_rx)) = loop_channel::<CounterClient>().await;

    println!("Creating counter server");
    let counter_obj = CounterObj::new();
    let (server, client) = CounterServer::new(counter_obj, 1);

    println!("Sending counter client");
    a_tx.send(client).await.unwrap();

    let client_task = async move {
        println!("Receiving counter client");
        let mut client = b_rx.recv().await.unwrap().unwrap();

        println!("add 20");
        client.increase(20).await.unwrap();

        println!("add 45");
        client.increase(45).await.unwrap();

        let value = client.value().await.unwrap();
        println!("value: {}", value);
        assert_eq!(value, 65);
    };

    let (_, counter_obj) = join!(client_task, server.serve());
    assert!(counter_obj.is_none());
}

#[tokio::test]
async fn simple_plus() {
    use remoc::rtc::Server;

    crate::init();
    let ((mut a_tx, _), (_, mut b_rx)) = loop_channel::<CounterClient>().await;

    println!("Creating counter server");
    let counter_obj = CounterObj::new();
    let (server, client) = CounterServer::new(counter_obj, 1);

    println!("Sending counter client");
    a_tx.send(client).await.unwrap();

    let client_task = async move {
        println!("Receiving counter client");
        let mut client = b_rx.recv().await.unwrap().unwrap();

        println!("add 20");
        client.increase(20).await.unwrap();

        println!("add 45");
        client.increase(45).await.unwrap();

        let value = client.value_plus(10).await.unwrap();
        println!("value: {}", value);
        assert_eq!(value, 75);
    };

    let (_, counter_obj) = join!(client_task, server.serve());
    assert!(counter_obj.is_none());
}
