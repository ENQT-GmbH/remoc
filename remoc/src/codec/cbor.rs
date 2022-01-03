// This attribute is necessary to avoid deprecation warnings on generated
// Serialize and Deserialize implementations.
#![allow(deprecated)]

use serde::{Deserialize, Serialize};

use super::{Codec, DeserializationError, SerializationError};

/// Legacy CBOR codec using [serde_cbor].
///
/// This codec uses [serde_cbor] which is [no longer maintained].
/// It is recommended to use `codec-ciborium` instead.
///
/// [no longer maintained]: https://rustsec.org/advisories/RUSTSEC-2021-0127
#[cfg_attr(docsrs, doc(cfg(feature = "codec-cbor")))]
#[deprecated(
    since = "0.9.10",
    note = "This uses serde_cbor, which is no longer maintained. \
            You should migrate to the Ciborium codec."
)]
#[derive(Clone, Serialize, Deserialize)]
pub struct Cbor;

#[allow(deprecated)]
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
