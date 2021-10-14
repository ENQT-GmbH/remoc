use serde::{Deserialize, Serialize};

use super::{CodecT, DeserializationError, SerializationError};

/// Bincode codec.
///
/// See [bincode] for details.
/// This uses the default function configuration.
#[derive(Clone, Serialize, Deserialize)]
pub struct BincodeCodec;

impl CodecT for BincodeCodec {
    fn serialize<Writer, Item>(writer: Writer, item: &Item) -> Result<(), super::SerializationError>
    where
        Writer: std::io::Write,
        Item: serde::Serialize,
    {
        bincode::serialize_into(writer, item).map_err(SerializationError::new)
    }

    fn deserialize<Reader, Item>(reader: Reader) -> Result<Item, super::DeserializationError>
    where
        Reader: std::io::Read,
        Item: serde::de::DeserializeOwned,
    {
        bincode::deserialize_from(reader).map_err(DeserializationError::new)
    }
}
