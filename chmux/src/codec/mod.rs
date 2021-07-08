//! Codecs for transforming messages into wire format.

use bytes::Bytes;
use serde::{de::DeserializeOwned, Serialize};
use std::{error::Error, fmt};

use crate::receiver::DataBuf;

/// Boxed error that is send, sync and static.
pub type BoxError = Box<dyn Error + Send + Sync + 'static>;

/// Serializes items into bytes.
pub trait Serializer<Item>: Send + fmt::Debug
where
    Item: Serialize,
{
    /// Serializes the specified item into the data format.
    fn serialize(&self, item: &Item) -> Result<Bytes, BoxError>;
}

/// Deserializes items from bytes.
pub trait Deserializer<Item>: Send + fmt::Debug
where
    Item: DeserializeOwned,
{
    /// Deserializes the specified data into an item.
    fn deserialize(&self, data: DataBuf) -> Result<Item, BoxError>;
}

/// Creates [Serializer]s and [Deserializer]s for converting any item type into the
/// multiplex message's content type.
pub trait CodecFactory: Clone + Send + fmt::Debug + 'static {
    /// Create a [Serializer] for the specified item type.
    fn serializer<Item: Serialize + 'static>(&self) -> Box<dyn Serializer<Item>>;

    /// Create a [Deserializer] for the specified item type.
    fn deserializer<Item: DeserializeOwned + 'static>(&self) -> Box<dyn Deserializer<Item>>;
}

#[cfg(feature = "json-codec")]
pub mod json;

#[cfg(feature = "bincode-codec")]
pub mod bincode;
