//! Codecs for transforming messages into wire format.

use serde::{de::DeserializeOwned, Serialize};
use std::error::Error;

use crate::MultiplexMsg;

/// Serializes items into a data format.
pub trait Serializer<Item, Data>: Send
where
    Item: Serialize,
{
    /// Serializes the specified item into the data format.
    fn serialize(&self, item: Item) -> Result<Data, Box<dyn Error + Send + 'static>>;
}

/// Deserializes items from a data format.
pub trait Deserializer<Item, Data>: Send
where
    Item: DeserializeOwned,
{
    /// Deserializes the specified data into an item.
    fn deserialize(&self, data: Data) -> Result<Item, Box<dyn Error + Send + 'static>>;
}

/// Creates `Serializer`s  and `Deserializer`s for converting any item type into the
/// multiplex message's content type.
pub trait ContentCodecFactory<Content>: Clone + Send {
    /// Create a `Serializer` for the specified item type.
    fn serializer<Item: Serialize + 'static>(&self) -> Box<dyn Serializer<Item, Content>>;

    /// Create a `Deserializer` for the specified item type.
    fn deserializer<Item: DeserializeOwned + 'static>(&self) -> Box<dyn Deserializer<Item, Content>>;
}

/// Creates `Serializer`s  and `Deserializer`s for converting a multiplex message
/// with a given content type into wire format.
pub trait TransportCodecFactory<Content: Serialize + DeserializeOwned + 'static, Data>: Clone + Send {
    /// Create a `Serializer` for a multiplex message.
    fn serializer(&self) -> Box<dyn Serializer<MultiplexMsg<Content>, Data>>;

    /// Create a `Deserializer` for the specified item type.
    fn deserializer(&self) -> Box<dyn Deserializer<MultiplexMsg<Content>, Data>>;
}

pub mod id;

#[cfg(feature = "json_codec")]
pub mod json;

#[cfg(feature = "bincode_codec")]
pub mod bincode;
