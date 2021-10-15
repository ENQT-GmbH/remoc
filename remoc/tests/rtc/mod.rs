use crate::loop_channel;
use remoc::rtc::{remote, CallError};

pub enum IncreaseError {
    Overflow,
    Call(CallError),
}

impl From<CallError> for IncreaseError {
    fn from(err: CallError) -> Self {
        Self::Call(err)
    }
}

#[remote]
pub trait Counter<Codec> {
    async fn value(&self) -> Result<u32, CallError>;
    async fn watch(&self) -> Result<watch::Receiver<u32, Codec>, CallError>;
    #[no_cancel]
    async fn increase(&mut self, #[serde(default)] by: u32) -> Result<(), IncreaseError>;
}

#[tokio::test]
async fn simple() {
    crate::init();
    let ((mut a_tx, _), (_, mut b_rx)) = loop_channel::<RFn<i16, Result<i16, CallError>>>().await;
}
