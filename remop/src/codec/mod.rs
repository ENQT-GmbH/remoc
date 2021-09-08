//! Codecs for transforming values into and from binary format.

use serde::{de::DeserializeOwned, Deserialize, Deserializer, Serialize};
use std::{
    error::Error,
    fmt,
    io::{Read, Write},
    sync::Arc,
};

/// Reference counted error that is send, sync, static and clone.
pub type ArcError = Arc<dyn Error + Send + Sync + 'static>;

/// An error consisting of a string message.
#[derive(Debug, Clone)]
pub struct ErrorMsg(pub String);

impl fmt::Display for ErrorMsg {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", &self.0)
    }
}

impl Error for ErrorMsg {}

/// Serialization error.
#[derive(Debug, Clone)]
pub struct SerializationError(pub ArcError);

impl SerializationError {
    /// Creates a new serialization error.
    pub fn new<E>(err: E) -> Self
    where
        E: Error + Send + Sync + 'static,
    {
        Self(Arc::new(err))
    }
}

impl fmt::Display for SerializationError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", &self.0)
    }
}

impl Error for SerializationError {}

impl Serialize for SerializationError {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let msg = self.0.to_string();
        msg.serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for SerializationError {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let msg = String::deserialize(deserializer)?;
        Ok(Self::new(ErrorMsg(msg)))
    }
}

/// Deserialization error.
#[derive(Debug, Clone)]
pub struct DeserializationError(pub ArcError);

impl DeserializationError {
    /// Creates a new deserialization error.
    pub fn new<E>(err: E) -> Self
    where
        E: Error + Send + Sync + 'static,
    {
        Self(Arc::new(err))
    }
}

impl fmt::Display for DeserializationError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", &self.0)
    }
}

impl Error for DeserializationError {}

impl Serialize for DeserializationError {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let msg = self.0.to_string();
        msg.serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for DeserializationError {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let msg = String::deserialize(deserializer)?;
        Ok(Self::new(ErrorMsg(msg)))
    }
}

/// Serializes and deserializes items from and to byte data.
pub trait CodecT: Send + Sync + Unpin + 'static {
    /// Serializes the specified item into the data format.
    fn serialize<Writer, Item>(writer: Writer, item: &Item) -> Result<(), SerializationError>
    where
        Writer: Write,
        Item: Serialize;

    /// Deserializes the specified data into an item.
    fn deserialize<Reader, Item>(reader: Reader) -> Result<Item, DeserializationError>
    where
        Reader: Read,
        Item: DeserializeOwned;
}

mod json;
pub mod serde_map;

pub use json::JsonCodec;
