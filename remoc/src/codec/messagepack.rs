use serde::{Deserialize, Serialize};

use super::{CodecT, DeserializationError, SerializationError};

/// MessagePack codec.
///
/// See [rmp_serde] for details.
/// This serializes sturctures as maps.
#[derive(Clone, Serialize, Deserialize)]
pub struct MessagepackCodec;

impl CodecT for MessagepackCodec {
    fn serialize<Writer, Item>(mut writer: Writer, item: &Item) -> Result<(), super::SerializationError>
    where
        Writer: std::io::Write,
        Item: serde::Serialize,
    {
        rmp_serde::encode::write_named(&mut writer, item).map_err(SerializationError::new)
    }

    fn deserialize<Reader, Item>(reader: Reader) -> Result<Item, super::DeserializationError>
    where
        Reader: std::io::Read,
        Item: serde::de::DeserializeOwned,
    {
        rmp_serde::decode::from_read(reader).map_err(DeserializationError::new)
    }
}
