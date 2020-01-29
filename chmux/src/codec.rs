use std::error::Error;

use serde::{de::DeserializeOwned, Serialize};

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

/// Creates `Serializer`s  and `Deserializer`s for the specified item type.
pub trait CodecFactory<Data>: Clone + Send {
    /// Create a `Serializer` for the specified item type.
    fn serializer<Item: Serialize + 'static>(&self) -> Box<dyn Serializer<Item, Data>>;

    /// Create a `Deserializer` for the specified item type.
    fn deserializer<Item: DeserializeOwned + 'static>(&self) -> Box<dyn Deserializer<Item, Data>>;
}
