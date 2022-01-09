// This should fail.

#[remoc::rtc::remote]
pub trait MethodLifetime
{
    async fn hello<'a>(&self) -> Result<Cow<'a, String>, remoc::rtc::CallError>;
}

#[remoc::rtc::remote]
pub trait TraitLifetime<'a>
{
    async fn hello(&self) -> Result<Cow<'a, String>, remoc::rtc::CallError>;
}
