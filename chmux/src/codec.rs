use std::error::Error;
use std::pin::Pin;

/// Serializes items into a data format.
pub trait Serializer<Item, Data> {
    /// Error type.
    type Error: Error + 'static;
    /// Serializes the specified item into the data format.
    fn serialize(self: Pin<&mut Self>, item: &Item) -> Result<Data, Self::Error>;
}

/// Deserializes items from a data format.
pub trait Deserializer<Item, Data> {
    /// Error type.
    type Error: Error + 'static;
    /// Deserializes the specified data into an item.
    fn deserialize(self: Pin<&mut Self>, data: &Data) -> Result<Item, Self::Error>;
}

/// Contains a serializer and matching deserializer.
pub trait Codec<SinkItem, StreamItem, Data> {
    /// Serializer type.
    type Serializer: Serializer<SinkItem, Data>;
    /// Deserializer type.
    type Deserializer: Deserializer<StreamItem, Data>;

    /// Splits the `Codec` into a `Serializer` and `Deserializer`.
    fn split(self) -> (Self::Serializer, Self::Deserializer);
}

