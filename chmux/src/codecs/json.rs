//! JSON codec.

use std::error::Error;
use std::marker::PhantomData;
use serde::{Serialize};
use serde::de::DeserializeOwned;
use serde_json::{Value, to_value, to_vec, from_slice, from_value};
use bytes::Bytes;

use crate::codec::{Serializer, Deserializer, CodecFactory};

pub struct JsonContentSerializer<Item> {
    _ghost_item: PhantomData<fn() -> Item>
}

impl<Item> Serializer<Item, Value> for JsonContentSerializer<Item>
where Item: Serialize 
{
    fn serialize(&self, item: Item) -> Result<Value, Box<dyn Error + Send + 'static>> {
        to_value(item).map_err(|err| Box::new(err) as Box<dyn Error + Send + 'static>)
    }
}

pub struct JsonContentDeserializer<Item> {
    _ghost_item: PhantomData<fn() -> Item>
}

impl<Item> Deserializer<Item, Value> for JsonContentDeserializer<Item>
where Item: DeserializeOwned
{
    fn deserialize(&self, data: Value) -> Result<Item, Box<dyn Error + Send + 'static>> {
        from_value(data).map_err(|err| Box::new(err) as Box<dyn Error + Send + 'static>)
    }
}


#[derive(Clone)]
pub struct JsonContentCodec {

}

impl JsonContentCodec {
    pub fn new() -> JsonContentCodec {
        JsonContentCodec {}
    }
}

impl CodecFactory<Value> for JsonContentCodec
{
    fn serializer<Item: Serialize + 'static>(&self) -> Box<dyn Serializer<Item, Value>> {
        Box::new(JsonContentSerializer {
            _ghost_item: PhantomData
        })
    }

    fn deserializer<Item: DeserializeOwned + 'static>(&self) -> Box<dyn Deserializer<Item, Value>> {
        Box::new(JsonContentDeserializer {
            _ghost_item: PhantomData
        })
    }
}





pub struct JsonTransportSerializer<Item> {
    _ghost_item: PhantomData<fn() -> Item>
}

impl<Item> Serializer<Item, Vec<u8>> for JsonTransportSerializer<Item>
where Item: Serialize 
{
    fn serialize(&self, item: Item) -> Result<Vec<u8>, Box<dyn Error + Send + 'static>> {
        to_vec(&item).map_err(|err| Box::new(err) as Box<dyn Error + Send + 'static>)
    }
}

impl<Item> Serializer<Item, Bytes> for JsonTransportSerializer<Item>
where Item: Serialize 
{
    fn serialize(&self, item: Item) -> Result<Bytes, Box<dyn Error + Send + 'static>> {
        to_vec(&item).map(Bytes::from).map_err(|err| Box::new(err) as Box<dyn Error + Send + 'static>)
    } 
}


pub struct JsonTransportDeserializer<Item> {
    _ghost_item: PhantomData<fn() -> Item>
}

impl<Item> Deserializer<Item, Vec<u8>> for JsonTransportDeserializer<Item>
where Item: DeserializeOwned
{
    fn deserialize(&self, data: Vec<u8>) -> Result<Item, Box<dyn Error + Send + 'static>> {
        from_slice(&data).map_err(|err| Box::new(err) as Box<dyn Error + Send + 'static>)
    }
}

impl<Item> Deserializer<Item, Bytes> for JsonTransportDeserializer<Item>
where Item: DeserializeOwned
{
    fn deserialize(&self, data: Bytes) -> Result<Item, Box<dyn Error + Send + 'static>> {
        from_slice(&data).map_err(|err| Box::new(err) as Box<dyn Error + Send + 'static>)
    } 
}


#[derive(Clone)]
pub struct JsonTransportCodec {

}

impl JsonTransportCodec {
    pub fn new() -> JsonTransportCodec {
        JsonTransportCodec {}
    }
}


impl CodecFactory<Vec<u8>> for JsonTransportCodec {
    fn serializer<Item: Serialize + 'static>(&self) -> Box<dyn Serializer<Item, Vec<u8>>> {
        Box::new(JsonTransportSerializer {
            _ghost_item: PhantomData
        })
    }

    fn deserializer<Item: DeserializeOwned + 'static>(&self) -> Box<dyn Deserializer<Item, Vec<u8>>> {
        Box::new(JsonTransportDeserializer {
            _ghost_item: PhantomData
        })
    }    
}

impl CodecFactory<Bytes> for JsonTransportCodec {
    fn serializer<Item: Serialize + 'static>(&self) -> Box<dyn Serializer<Item, Bytes>> {
        Box::new(JsonTransportSerializer {
            _ghost_item: PhantomData
        })
    }

    fn deserializer<Item: DeserializeOwned + 'static>(&self) -> Box<dyn Deserializer<Item, Bytes>> {
        Box::new(JsonTransportDeserializer {
            _ghost_item: PhantomData
        })
    }    
}
