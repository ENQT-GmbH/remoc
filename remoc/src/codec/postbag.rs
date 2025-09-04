use serde::{Deserialize, Serialize};

use super::{Codec, DeserializationError, SerializationError};

/// Postbag codec.
///
/// See [postbag] for details.
/// This serializes and deserializes *with* identifiers.
#[cfg_attr(docsrs, doc(cfg(feature = "codec-postbag")))]
#[derive(Clone, Serialize, Deserialize)]
pub struct Postbag;

type Config = postbag::Config<true>;

impl Codec for Postbag {
    fn serialize<Writer, Item>(writer: Writer, item: &Item) -> Result<(), super::SerializationError>
    where
        Writer: std::io::Write,
        Item: serde::Serialize,
    {
        postbag::to_io_with_cfg::<_, _, Config>(item, writer).map_err(SerializationError::new)?;
        Ok(())
    }

    fn deserialize<Reader, Item>(reader: Reader) -> Result<Item, super::DeserializationError>
    where
        Reader: std::io::Read,
        Item: serde::de::DeserializeOwned,
    {
        let (value, _reader) =
            postbag::from_io_with_cfg::<_, _, Config>(reader).map_err(DeserializationError::new)?;
        Ok(value)
    }
}

/// Postbag slim codec.
///
/// See [postbag] for details.
/// This serializes and deserializes *without* identifiers.
#[cfg_attr(docsrs, doc(cfg(feature = "codec-postbag")))]
#[derive(Clone, Serialize, Deserialize)]
pub struct PostbagSlim;

type SlimConfig = postbag::Config<false>;

impl Codec for PostbagSlim {
    fn serialize<Writer, Item>(writer: Writer, item: &Item) -> Result<(), super::SerializationError>
    where
        Writer: std::io::Write,
        Item: serde::Serialize,
    {
        postbag::to_io_with_cfg::<_, _, SlimConfig>(item, writer).map_err(SerializationError::new)?;
        Ok(())
    }

    fn deserialize<Reader, Item>(reader: Reader) -> Result<Item, super::DeserializationError>
    where
        Reader: std::io::Read,
        Item: serde::de::DeserializeOwned,
    {
        let (value, _reader) =
            postbag::from_io_with_cfg::<_, _, SlimConfig>(reader).map_err(DeserializationError::new)?;
        Ok(value)
    }
}
