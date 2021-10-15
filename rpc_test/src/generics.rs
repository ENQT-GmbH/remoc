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

pub trait CheckedAddable
where
    Self: Sized,
{
    fn checked_add(self, rhs: Self) -> Option<Self>;
}

impl CheckedAddable for u8 {
    fn checked_add(self, rhs: Self) -> Option<Self> {
        self.checked_add(rhs)
    }
}

#[remoc::rtc::remote]
pub trait GenericCounter<T>
where
    T: remoc::RemoteSend + CheckedAddable + Clone + Default + Sync + 'static,
{
    async fn value(&self) -> Result<T, remoc::rtc::CallError>;
    async fn watch(&mut self) -> Result<remoc::rch::watch::Receiver<T>, remoc::rtc::CallError>;
    #[no_cancel]
    async fn increase(&mut self, #[serde(default)] by: T) -> Result<(), IncreaseError>;
}

pub struct GenericCounterObj<T> {
    value: T,
    watchers: Vec<remoc::rch::watch::Sender<T>>,
}

impl<T> GenericCounterObj<T>
where
    T: Default,
{
    pub fn new() -> Self {
        Self { value: T::default(), watchers: Vec::new() }
    }
}

#[remoc::rtc::async_trait]
impl<T> GenericCounter<T> for GenericCounterObj<T>
where
    T: remoc::RemoteSend + CheckedAddable + Clone + Default + Sync,
{
    async fn value(&self) -> Result<T, remoc::rtc::CallError> {
        Ok(self.value.clone())
    }

    async fn watch(&mut self) -> Result<remoc::rch::watch::Receiver<T>, remoc::rtc::CallError> {
        let (tx, rx) = remoc::rch::watch::channel(self.value.clone());
        self.watchers.push(tx);
        Ok(rx)
    }

    async fn increase(&mut self, by: T) -> Result<(), IncreaseError> {
        match self.value.clone().checked_add(by) {
            Some(new_value) => self.value = new_value,
            None => return Err(IncreaseError::Overflow),
        }

        for watch in &self.watchers {
            let _ = watch.send(self.value.clone());
        }

        Ok(())
    }
}
