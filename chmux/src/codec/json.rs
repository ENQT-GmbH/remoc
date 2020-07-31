//! JSON codec.

use bytes::Bytes;
use serde::{de::DeserializeOwned, Serialize};
use serde_json::{from_slice, from_value, to_value, to_vec, Value};
use std::{error::Error, marker::PhantomData};

use crate::{
    codec::{ContentCodecFactory, Deserializer, Serializer, TransportCodecFactory},
    MultiplexMsg,
};

// ============================================================================
// JSON content codec
// ============================================================================

/// Serializes message content into JSON format
pub struct JsonContentSerializer<Item> {
    _ghost_item: PhantomData<fn() -> Item>,
}

impl<Item> Serializer<Item, Value> for JsonContentSerializer<Item>
where
    Item: Serialize,
{
    fn serialize(&self, item: Item) -> Result<Value, Box<dyn Error + Send + 'static>> {
        to_value(item).map_err(|err| Box::new(err) as Box<dyn Error + Send + 'static>)
    }
}

/// Deserializes message content from JSON format.
pub struct JsonContentDeserializer<Item> {
    _ghost_item: PhantomData<fn() -> Item>,
}

impl<Item> Deserializer<Item, Value> for JsonContentDeserializer<Item>
where
    Item: DeserializeOwned,
{
    fn deserialize(&self, data: Value) -> Result<Item, Box<dyn Error + Send + 'static>> {
        from_value(data).map_err(|err| Box::new(err) as Box<dyn Error + Send + 'static>)
    }
}

/// JSON codec for message content.
#[derive(Clone)]
pub struct JsonContentCodec {}

impl JsonContentCodec {
    /// Creates a new JSON codec for messages content.
    pub fn new() -> JsonContentCodec {
        JsonContentCodec {}
    }
}

impl ContentCodecFactory<Value> for JsonContentCodec {
    fn serializer<Item: Serialize + 'static>(&self) -> Box<dyn Serializer<Item, Value>> {
        Box::new(JsonContentSerializer { _ghost_item: PhantomData })
    }

    fn deserializer<Item: DeserializeOwned + 'static>(&self) -> Box<dyn Deserializer<Item, Value>> {
        Box::new(JsonContentDeserializer { _ghost_item: PhantomData })
    }
}

// ============================================================================
// JSON transport codec
// ============================================================================

/// Serializes messages into JSON format for transportation.
pub struct JsonTransportSerializer<Item> {
    _ghost_item: PhantomData<fn() -> Item>,
}

impl<Item> Serializer<Item, Vec<u8>> for JsonTransportSerializer<Item>
where
    Item: Serialize,
{
    fn serialize(&self, item: Item) -> Result<Vec<u8>, Box<dyn Error + Send + 'static>> {
        to_vec(&item).map_err(|err| Box::new(err) as Box<dyn Error + Send + 'static>)
    }
}

impl<Item> Serializer<Item, Bytes> for JsonTransportSerializer<Item>
where
    Item: Serialize,
{
    fn serialize(&self, item: Item) -> Result<Bytes, Box<dyn Error + Send + 'static>> {
        to_vec(&item).map(Bytes::from).map_err(|err| Box::new(err) as Box<dyn Error + Send + 'static>)
    }
}

/// Deserializes JSON data received from transportation into messages.
pub struct JsonTransportDeserializer<Item> {
    _ghost_item: PhantomData<fn() -> Item>,
}

impl<Item> Deserializer<Item, Vec<u8>> for JsonTransportDeserializer<Item>
where
    Item: DeserializeOwned,
{
    fn deserialize(&self, data: Vec<u8>) -> Result<Item, Box<dyn Error + Send + 'static>> {
        from_slice(&data).map_err(|err| Box::new(err) as Box<dyn Error + Send + 'static>)
    }
}

impl<Item> Deserializer<Item, Bytes> for JsonTransportDeserializer<Item>
where
    Item: DeserializeOwned,
{
    fn deserialize(&self, data: Bytes) -> Result<Item, Box<dyn Error + Send + 'static>> {
        from_slice(&data).map_err(|err| Box::new(err) as Box<dyn Error + Send + 'static>)
    }
}

/// JSON codec for message transport.
#[derive(Clone)]
pub struct JsonTransportCodec {}

impl JsonTransportCodec {
    /// Creates a new JSON codec for messages transport.
    pub fn new() -> JsonTransportCodec {
        JsonTransportCodec {}
    }
}

impl TransportCodecFactory<Value, Vec<u8>> for JsonTransportCodec {
    fn serializer(&self) -> Box<dyn Serializer<MultiplexMsg<Value>, Vec<u8>>> {
        Box::new(JsonTransportSerializer { _ghost_item: PhantomData })
    }

    fn deserializer(&self) -> Box<dyn Deserializer<MultiplexMsg<Value>, Vec<u8>>> {
        Box::new(JsonTransportDeserializer { _ghost_item: PhantomData })
    }
}

impl TransportCodecFactory<Value, Bytes> for JsonTransportCodec {
    fn serializer(&self) -> Box<dyn Serializer<MultiplexMsg<Value>, Bytes>> {
        Box::new(JsonTransportSerializer { _ghost_item: PhantomData })
    }

    fn deserializer(&self) -> Box<dyn Deserializer<MultiplexMsg<Value>, Bytes>> {
        Box::new(JsonTransportDeserializer { _ghost_item: PhantomData })
    }
}
