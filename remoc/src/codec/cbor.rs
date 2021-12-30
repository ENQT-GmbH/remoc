use serde::{Deserialize, Serialize};

use super::{Codec, DeserializationError, SerializationError};

/// CBOR codec.
///
/// See [ciborium] for details.
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
        ciborium::ser::into_writer(item, writer).map_err(SerializationError::new)
    }

    #[inline]
    fn deserialize<Reader, Item>(reader: Reader) -> Result<Item, super::DeserializationError>
    where
        Reader: std::io::Read,
        Item: serde::de::DeserializeOwned,
    {
        ciborium::de::from_reader(reader).map_err(DeserializationError::new)
    }
}
