//! JSON codec.

use bytes::{Buf, BufMut, Bytes, BytesMut};
use serde::{de::DeserializeOwned, Serialize};
use std::{any::type_name, fmt, marker::PhantomData};

use super::BoxError;
use crate::{
    codec::{CodecFactory, Deserializer, Serializer},
    receiver::DataBuf,
};

/// Serializes items into JSON format.
pub struct JsonSerializer<Item> {
    _ghost_item: PhantomData<fn() -> Item>,
}

impl<Item> fmt::Debug for JsonSerializer<Item> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct(&format!("JsonSerializer<{}>", type_name::<Item>())).finish()
    }
}

impl<Item> Serializer<Item> for JsonSerializer<Item>
where
    Item: Serialize,
{
    fn serialize(&self, item: &Item) -> Result<Bytes, BoxError> {
        let mut writer = BytesMut::new().writer();
        serde_json::to_writer(&mut writer, &item).map_err(|err| Box::new(err) as BoxError)?;
        Ok(writer.into_inner().freeze())
    }
}

/// Deserializes items from JSON format.
pub struct JsonDeserializer<Item> {
    _ghost_item: PhantomData<fn() -> Item>,
}

impl<Item> fmt::Debug for JsonDeserializer<Item> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct(&format!("JsonDeserializer<{}>", type_name::<Item>())).finish()
    }
}

impl<Item> Deserializer<Item> for JsonDeserializer<Item>
where
    Item: DeserializeOwned,
{
    fn deserialize(&self, data: DataBuf) -> Result<Item, BoxError> {
        serde_json::from_reader(data.reader()).map_err(|err| Box::new(err) as BoxError)
    }
}

/// JSON codec.
#[derive(Clone, Debug)]
pub struct JsonCodec {}

impl JsonCodec {
    /// Creates a new JSON codec.
    pub fn new() -> Self {
        Self {}
    }
}

impl Default for JsonCodec {
    fn default() -> Self {
        Self::new()
    }
}

impl CodecFactory for JsonCodec {
    fn serializer<Item: Serialize + 'static>(&self) -> Box<dyn Serializer<Item>> {
        Box::new(JsonSerializer { _ghost_item: PhantomData })
    }

    fn deserializer<Item: DeserializeOwned + 'static>(&self) -> Box<dyn Deserializer<Item>> {
        Box::new(JsonDeserializer { _ghost_item: PhantomData })
    }
}
