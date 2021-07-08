//! Bincode codec.

#[allow(deprecated)]
use bincode::Config;
use bytes::{Buf, BufMut, Bytes, BytesMut};
use serde::{de::DeserializeOwned, Serialize};
use std::{any::type_name, fmt, marker::PhantomData, sync::Arc};

use super::BoxError;
use crate::{
    codec::{CodecFactory, Deserializer, Serializer},
    receiver::DataBuf,
};

/// Serializes items into Bincode format.
pub struct BincodeSerializer<Item> {
    #[allow(deprecated)]
    config: Arc<Config>,
    _ghost_item: PhantomData<fn() -> Item>,
}

impl<Item> fmt::Debug for BincodeSerializer<Item> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct(&format!("BincodeSerializer<{}>", type_name::<Item>()))
            .field("config", &self.config)
            .finish()
    }
}

impl<Item> Serializer<Item> for BincodeSerializer<Item>
where
    Item: Serialize,
{
    fn serialize(&self, item: &Item) -> Result<Bytes, BoxError> {
        let mut writer = BytesMut::new().writer();
        self.config.serialize_into(&mut writer, &item).map_err(|err| Box::new(err) as BoxError)?;
        Ok(writer.into_inner().freeze())
    }
}

/// Deserializes items from Bincode format.
pub struct BincodeDeserializer<Item> {
    #[allow(deprecated)]
    config: Arc<Config>,
    _ghost_item: PhantomData<fn() -> Item>,
}

impl<Item> fmt::Debug for BincodeDeserializer<Item> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct(&format!("BincodeDeserializer<{}>", type_name::<Item>()))
            .field("config", &self.config)
            .finish()
    }
}

impl<Item> Deserializer<Item> for BincodeDeserializer<Item>
where
    Item: DeserializeOwned,
{
    fn deserialize(&self, data: DataBuf) -> Result<Item, BoxError> {
        self.config.deserialize_from(data.reader()).map_err(|err| Box::new(err) as BoxError)
    }
}

/// Bincode codec.
#[derive(Clone, Debug)]
pub struct BincodeCodec {
    #[allow(deprecated)]
    config: Arc<Config>,
}

impl BincodeCodec {
    /// Creates a new Bincode codec with the default configuration.
    pub fn new() -> Self {
        #[allow(deprecated)]
        Self::with_config(bincode::config())
    }

    /// Creates a new Bincode codec with the specified configuration.
    #[allow(deprecated)]
    pub fn with_config(config: Config) -> Self {
        Self { config: Arc::new(config) }
    }
}

impl Default for BincodeCodec {
    fn default() -> Self {
        Self::new()
    }
}

impl CodecFactory for BincodeCodec {
    fn serializer<Item: Serialize + 'static>(&self) -> Box<dyn Serializer<Item>> {
        Box::new(BincodeSerializer { config: self.config.clone(), _ghost_item: PhantomData })
    }

    fn deserializer<Item: DeserializeOwned + 'static>(&self) -> Box<dyn Deserializer<Item>> {
        Box::new(BincodeDeserializer { config: self.config.clone(), _ghost_item: PhantomData })
    }
}
