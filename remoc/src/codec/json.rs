use serde::{Deserialize, Serialize};

use super::{Codec, DeserializationError, SerializationError};

/// JSON codec.
///
/// See [serde_json] for details.
#[derive(Clone, Serialize, Deserialize)]
pub struct Json;

impl Codec for Json {
    fn serialize<Writer, Item>(writer: Writer, item: &Item) -> Result<(), super::SerializationError>
    where
        Writer: std::io::Write,
        Item: serde::Serialize,
    {
        serde_json::to_writer(writer, item).map_err(SerializationError::new)
    }

    fn deserialize<Reader, Item>(reader: Reader) -> Result<Item, super::DeserializationError>
    where
        Reader: std::io::Read,
        Item: serde::de::DeserializeOwned,
    {
        serde_json::from_reader(reader).map_err(DeserializationError::new)
    }
}
