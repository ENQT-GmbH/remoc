use std::error::Error;

/// Serializes items into a data format.
pub trait Serializer<Item, Data> : Send {
    /// Serializes the specified item into the data format.
    fn serialize(&self, item: Item) -> Result<Data, Box<dyn Error + Send + 'static>>;
}

/// Deserializes items from a data format.
pub trait Deserializer<Item, Data> : Send {
    /// Deserializes the specified data into an item.
    fn deserialize(&self, data: Data) -> Result<Item, Box<dyn Error + Send + 'static>>;
}

/// Creates `Serializer`s  and `Deserializer`s for the specified item type.
pub trait CodecFactory<Data> : Clone + Send {
    /// Create a `Serializer` for the specified item type.
    fn serializer<Item>(&self) -> Box<dyn Serializer<Item, Data>>;

    /// Create a `Deserializer` for the specified item type.
    fn deserializer<Item>(&self) -> Box<dyn Deserializer<Item, Data>>;
}
