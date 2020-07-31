//! Identity transport codec.

use serde::{de::DeserializeOwned, Serialize};
use std::{error::Error, marker::PhantomData};

use crate::{
    codec::{Deserializer, Serializer},
    MultiplexMsg, TransportCodecFactory,
};

/// Passes messages as-is to transport.
pub struct IdTransportSerializer<Item> {
    _ghost_item: PhantomData<fn() -> Item>,
}

impl<Item> Serializer<Item, Item> for IdTransportSerializer<Item>
where
    Item: Serialize,
{
    fn serialize(&self, item: Item) -> Result<Item, Box<dyn Error + Send + 'static>> {
        Ok(item)
    }
}

/// Passes messages as-is from transport.
pub struct IdTransportDeserializer<Item> {
    _ghost_item: PhantomData<fn() -> Item>,
}

impl<Item> Deserializer<Item, Item> for IdTransportDeserializer<Item>
where
    Item: DeserializeOwned,
{
    fn deserialize(&self, data: Item) -> Result<Item, Box<dyn Error + Send + 'static>> {
        Ok(data)
    }
}

/// Identity codec for message transport.
#[derive(Clone)]
pub struct IdTransportCodec {}

impl IdTransportCodec {
    /// Creates a new identity codec for messages transport.
    pub fn new() -> IdTransportCodec {
        IdTransportCodec {}
    }
}

impl<Content: Serialize + DeserializeOwned + 'static> TransportCodecFactory<Content, MultiplexMsg<Content>>
    for IdTransportCodec
{
    fn serializer(&self) -> Box<dyn Serializer<MultiplexMsg<Content>, MultiplexMsg<Content>>> {
        Box::new(IdTransportSerializer { _ghost_item: PhantomData })
    }

    fn deserializer(&self) -> Box<dyn Deserializer<MultiplexMsg<Content>, MultiplexMsg<Content>>> {
        Box::new(IdTransportDeserializer { _ghost_item: PhantomData })
    }
}
