//mod out;

/// My error
#[derive(serde::Serialize, serde::Deserialize)]
pub enum MyError {
    Error1,
    Call(remoc::rtc::CallError),
}

impl From<remoc::rtc::CallError> for MyError {
    fn from(err: remoc::rtc::CallError) -> Self {
        Self::Call(err)
    }
}

/// My service
#[remoc::rtc::remote]
pub trait MyService {
    /// Const fn docs.
    async fn const_fn(
        &self, arg1: String, arg2: u16, arg3: remoc::rch::mpsc::Sender<String>,
    ) -> Result<u32, MyError>;

    /// Mut fn docs.
    async fn mut_fn(&mut self, arg1: Vec<String>) -> Result<(), MyError>;
}

pub struct MyObject {
    field1: String,
}

#[remoc::rtc::async_trait]
impl MyService for MyObject {
    async fn const_fn(
        &self, arg1: String, arg2: u16, arg3: remoc::rch::mpsc::Sender<String>,
    ) -> Result<u32, MyError> {
        println!("arg1: {}, arg2: {}", arg1, arg2);
        arg3.send("Hallo".to_string()).await.unwrap();
        Ok(123)
    }

    async fn mut_fn(&mut self, arg1: Vec<String>) -> Result<(), MyError> {
        self.field1 = arg1.join(",");
        Err(MyError::Error1)
    }
}

pub async fn do_test() {
    let obj = MyObject { field1: String::new() };
}

/// My generic service
#[remoc::rtc::remote]
pub trait MyGenericService<T>
where
    T: ::remoc::RemoteSend,
{
    /// Mut fn docs.
    async fn mut_fn(&mut self, arg1: T) -> Result<(), MyError>;
}

// Okay, we need to clear up that generic chaos.
// Do we need a generic type on the trait?
// Probably not necessarily, but?
// Yes, the codec of the request encoding could be determined
// wholely by the server and client types, that are generic over codec.
