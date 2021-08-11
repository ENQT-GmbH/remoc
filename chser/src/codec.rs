//! Codecs for transforming messages into wire format.

use bytes::Bytes;
use chmux::DataBuf;
use serde::{de::DeserializeOwned, Serialize};
use std::{error::Error, fmt};

/// Boxed error that is send, sync and static.
pub type BoxError = Box<dyn Error + Send + Sync + 'static>;

/// Serialization error.
#[derive(Debug)]
pub struct SerializationError(pub BoxError);

impl SerializationError {
    /// Creates a new serialization error.
    pub fn new<E>(err: E) -> Self
    where
        E: Error + Send + Sync + 'static,
    {
        Self(Box::new(err))
    }
}

impl fmt::Display for SerializationError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", &self.0)
    }
}

impl Error for SerializationError {}

/// Deserialization error.
#[derive(Debug)]
pub struct DeserializationError(pub BoxError);

impl DeserializationError {
    /// Creates a new deserialization error.
    pub fn new<E>(err: E) -> Self
    where
        E: Error + Send + Sync + 'static,
    {
        Self(Box::new(err))
    }
}

impl fmt::Display for DeserializationError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", &self.0)
    }
}

impl Error for DeserializationError {}

/// Serializes and deserializes items from and to byte data.
pub trait CodecT: Clone + Send + Sync + fmt::Debug + 'static {
    /// Serializes the specified item into the data format.
    fn serialize<Item>(item: &Item) -> Result<Bytes, SerializationError>
    where
        Item: Serialize;

    /// Deserializes the specified data into an item.
    fn deserialize<Item>(data: DataBuf) -> Result<Item, DeserializationError>
    where
        Item: DeserializeOwned;
}
