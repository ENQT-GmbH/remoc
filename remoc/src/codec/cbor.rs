use serde::{Deserialize, Serialize};

use super::{Codec, DeserializationError, SerializationError};

/// CBOR codec.
///
/// See [serde_cbor] for details.
#[cfg_attr(docsrs, doc(cfg(feature = "codec-cbor")))]
#[derive(Clone, Serialize, Deserialize)]
pub struct Cbor;

impl Codec for Cbor {
    #[inline]
    fn serialize<Writer, Item>(writer: Writer, item: &Item) -> Result<(), super::SerializationError>
    where
        Writer: std::io::Write,
        Item: serde::Serialize,
    {
        serde_cbor::to_writer(writer, item).map_err(SerializationError::new)
    }

    #[inline]
    fn deserialize<Reader, Item>(reader: Reader) -> Result<Item, super::DeserializationError>
    where
        Reader: std::io::Read,
        Item: serde::de::DeserializeOwned,
    {
        serde_cbor::from_reader(reader).map_err(DeserializationError::new)
    }
}
