//! Bincode codec.

#[allow(deprecated)]
use bincode::Config;
use bytes::Bytes;
use serde::{de::DeserializeOwned, Serialize};
use std::{error::Error, marker::PhantomData, sync::Arc};

use crate::{
    codec::{ContentCodecFactory, Deserializer, Serializer, TransportCodecFactory},
    MultiplexMsg,
};

// ============================================================================
// Bincode content codec
// ============================================================================

/// Serializes message content into Bincode format
pub struct BincodeContentSerializer<Item> {
    #[allow(deprecated)]
    config: Arc<Config>,
    _ghost_item: PhantomData<fn() -> Item>,
}

impl<Item> Serializer<Item, Vec<u8>> for BincodeContentSerializer<Item>
where
    Item: Serialize,
{
    fn serialize(&self, item: Item) -> Result<Vec<u8>, Box<dyn Error + Send + Sync + 'static>> {
        self.config.serialize(&item).map_err(|err| Box::new(err) as Box<dyn Error + Send + Sync + 'static>)
    }
}

/// Deserializes message content from Bincode format.
pub struct BincodeContentDeserializer<Item> {
    #[allow(deprecated)]
    config: Arc<Config>,
    _ghost_item: PhantomData<fn() -> Item>,
}

impl<Item> Deserializer<Item, Vec<u8>> for BincodeContentDeserializer<Item>
where
    Item: DeserializeOwned,
{
    fn deserialize(&self, data: Vec<u8>) -> Result<Item, Box<dyn Error + Send + Sync + 'static>> {
        self.config.deserialize(&data).map_err(|err| Box::new(err) as Box<dyn Error + Send + Sync + 'static>)
    }
}

/// Bincode codec for message content.
#[derive(Clone)]
pub struct BincodeContentCodec {
    #[allow(deprecated)]
    config: Arc<Config>,
}

impl BincodeContentCodec {
    /// Creates a new Bincode codec for messages content with the default configuration.
    pub fn new() -> Self {
        #[allow(deprecated)]
        Self::with_config(bincode::config())
    }

    /// Creates a new Bincode codec for messages content with the specified configuration.
    #[allow(deprecated)]
    pub fn with_config(config: Config) -> Self {
        Self { config: Arc::new(config) }
    }
}

impl Default for BincodeContentCodec {
    fn default() -> Self {
        Self::new()
    }
}

impl ContentCodecFactory<Vec<u8>> for BincodeContentCodec {
    fn serializer<Item: Serialize + 'static>(&self) -> Box<dyn Serializer<Item, Vec<u8>>> {
        Box::new(BincodeContentSerializer { config: self.config.clone(), _ghost_item: PhantomData })
    }

    fn deserializer<Item: DeserializeOwned + 'static>(&self) -> Box<dyn Deserializer<Item, Vec<u8>>> {
        Box::new(BincodeContentDeserializer { config: self.config.clone(), _ghost_item: PhantomData })
    }
}

// ============================================================================
// Bincode transport codec
// ============================================================================

/// Serializes messages into Bincode format for transportation.
pub struct BincodeTransportSerializer<Item> {
    #[allow(deprecated)]
    config: Arc<Config>,
    _ghost_item: PhantomData<fn() -> Item>,
}

impl<Item> Serializer<Item, Vec<u8>> for BincodeTransportSerializer<Item>
where
    Item: Serialize,
{
    fn serialize(&self, item: Item) -> Result<Vec<u8>, Box<dyn Error + Send + Sync + 'static>> {
        self.config.serialize(&item).map_err(|err| Box::new(err) as Box<dyn Error + Send + Sync + 'static>)
    }
}

impl<Item> Serializer<Item, Bytes> for BincodeTransportSerializer<Item>
where
    Item: Serialize,
{
    fn serialize(&self, item: Item) -> Result<Bytes, Box<dyn Error + Send + Sync + 'static>> {
        self.config
            .serialize(&item)
            .map(Bytes::from)
            .map_err(|err| Box::new(err) as Box<dyn Error + Send + Sync + 'static>)
    }
}

/// Deserializes Bincode data received from transportation into messages.
pub struct BincodeTransportDeserializer<Item> {
    #[allow(deprecated)]
    config: Arc<Config>,
    _ghost_item: PhantomData<fn() -> Item>,
}

impl<Item> Deserializer<Item, Vec<u8>> for BincodeTransportDeserializer<Item>
where
    Item: DeserializeOwned,
{
    fn deserialize(&self, data: Vec<u8>) -> Result<Item, Box<dyn Error + Send + Sync + 'static>> {
        self.config.deserialize(&data).map_err(|err| Box::new(err) as Box<dyn Error + Send + Sync + 'static>)
    }
}

impl<Item> Deserializer<Item, Bytes> for BincodeTransportDeserializer<Item>
where
    Item: DeserializeOwned,
{
    fn deserialize(&self, data: Bytes) -> Result<Item, Box<dyn Error + Send + Sync + 'static>> {
        self.config.deserialize(&data).map_err(|err| Box::new(err) as Box<dyn Error + Send + Sync + 'static>)
    }
}

/// Bincode codec for message transport.
#[derive(Clone)]
pub struct BincodeTransportCodec {
    #[allow(deprecated)]
    config: Arc<bincode::Config>,
}

impl BincodeTransportCodec {
    /// Creates a new Bincode codec for messages transport.
    #[allow(deprecated)]
    pub fn new() -> Self {
        #[allow(deprecated)]
        Self::with_config(bincode::config())
    }

    /// Creates a new Bincode codec for messages transport with the specified configuration.
    #[allow(deprecated)]
    pub fn with_config(config: Config) -> Self {
        Self { config: Arc::new(config) }
    }
}

impl Default for BincodeTransportCodec {
    fn default() -> Self {
        Self::new()
    }
}

impl TransportCodecFactory<Vec<u8>, Vec<u8>> for BincodeTransportCodec {
    fn serializer(&self) -> Box<dyn Serializer<MultiplexMsg<Vec<u8>>, Vec<u8>>> {
        Box::new(BincodeTransportSerializer { config: self.config.clone(), _ghost_item: PhantomData })
    }

    fn deserializer(&self) -> Box<dyn Deserializer<MultiplexMsg<Vec<u8>>, Vec<u8>>> {
        Box::new(BincodeTransportDeserializer { config: self.config.clone(), _ghost_item: PhantomData })
    }
}

impl TransportCodecFactory<Vec<u8>, Bytes> for BincodeTransportCodec {
    fn serializer(&self) -> Box<dyn Serializer<MultiplexMsg<Vec<u8>>, Bytes>> {
        Box::new(BincodeTransportSerializer { config: self.config.clone(), _ghost_item: PhantomData })
    }

    fn deserializer(&self) -> Box<dyn Deserializer<MultiplexMsg<Vec<u8>>, Bytes>> {
        Box::new(BincodeTransportDeserializer { config: self.config.clone(), _ghost_item: PhantomData })
    }
}
