//! Codecs for transforming messages into wire format.

#[cfg(feature="json_codec")]
pub mod json;

#[cfg(feature="bincode_codec")]
pub mod bincode;

