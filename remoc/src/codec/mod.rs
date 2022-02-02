//! Codecs for transforming values into and from binary wire format.
//!
//! All codecs in this module are wrappers around the [serde] crates implementing the
//! data representations.
//! Thus you should refer to the corresponding crate documentation for information
//! about limitations and backward as well as forward compatibility.
//!
//! # Crate features
//!
//! Each codec is gated by the corresponding crate feature `codec-*`, i.e.
//! the JSON codec is only available if the crate features `codec-json` is enabled.
//! The crate feature `full-codecs` enables all codecs.
//!
//! The default codec, named [Default](struct@Default), can be selected by enabling the
//! appropriate `default-codec-*` crate feature.
//! For example, if you want to use the JSON codec by default, enable the crate feature
//! `default-codec-json`.
//! Only one default codec feature must be enabled, otherwise a compile error will occur.
//! The default codec should only be selected by an application and not a library crate
//! that uses Remoc.
//! Otherwise a conflict between multiple libraries that depend upon different default
//! codecs will occur.
//!
//! The following features select the default codec.
//!
//!   * `default-codec-bincode` -- enables and selects Bincode as the default codec
//!   * `default-codec-ciborium` -- enables and selects CBOR as the default codec
//!   * `default-codec-json` -- enables and selects JSON as the default codec
//!   * `default-codec-message-pack` -- enables selects MessagePack as the default codec
//!
//! By default the JSON codec is enabled and the default, i.e. the `default-codec-json`
//! crate feature is enabled.
//! Thus to change the default codec, you must specify `default-features = false` when
//! referencing Remoc in your `Cargo.toml`.
//!

use serde::{de::DeserializeOwned, Deserialize, Deserializer, Serialize};
use std::{
    error::Error,
    fmt,
    io::{Read, Write},
    sync::Arc,
};

/// Reference counted error that is send, sync, static and clone.
pub type ArcError = Arc<dyn Error + Send + Sync + 'static>;

/// An error consisting of a string message.
#[derive(Debug, Clone)]
pub(crate) struct ErrorMsg(pub String);

impl fmt::Display for ErrorMsg {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", &self.0)
    }
}

impl Error for ErrorMsg {}

/// Serialization error.
#[derive(Debug, Clone)]
pub struct SerializationError(pub ArcError);

impl SerializationError {
    /// Creates a new serialization error.
    pub fn new<E>(err: E) -> Self
    where
        E: Error + Send + Sync + 'static,
    {
        Self(Arc::new(err))
    }
}

impl fmt::Display for SerializationError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", &self.0)
    }
}

impl Error for SerializationError {}

impl Serialize for SerializationError {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let msg = self.0.to_string();
        msg.serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for SerializationError {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let msg = String::deserialize(deserializer)?;
        Ok(Self::new(ErrorMsg(msg)))
    }
}

/// Deserialization error.
#[derive(Debug, Clone)]
pub struct DeserializationError(pub ArcError);

impl DeserializationError {
    /// Creates a new deserialization error.
    pub fn new<E>(err: E) -> Self
    where
        E: Error + Send + Sync + 'static,
    {
        Self(Arc::new(err))
    }
}

impl fmt::Display for DeserializationError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", &self.0)
    }
}

impl Error for DeserializationError {}

impl Serialize for DeserializationError {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let msg = self.0.to_string();
        msg.serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for DeserializationError {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let msg = String::deserialize(deserializer)?;
        Ok(Self::new(ErrorMsg(msg)))
    }
}

/// Serializes and deserializes items from and to byte data.
pub trait Codec: Send + Sync + Serialize + for<'de> Deserialize<'de> + Clone + Unpin + 'static {
    /// Serializes the specified item into the data format.
    fn serialize<Writer, Item>(writer: Writer, item: &Item) -> Result<(), SerializationError>
    where
        Writer: Write,
        Item: Serialize;

    /// Deserializes the specified data into an item.
    fn deserialize<Reader, Item>(reader: Reader) -> Result<Item, DeserializationError>
    where
        Reader: Read,
        Item: DeserializeOwned;
}

pub mod map;

// ============================================================================
// Codecs
// ============================================================================

#[cfg(feature = "codec-bincode")]
mod bincode;
#[cfg(feature = "codec-bincode")]
pub use self::bincode::Bincode;
#[cfg(feature = "default-codec-bincode")]
#[doc(no_inline)]
pub use self::bincode::Bincode as Default;

#[cfg(feature = "codec-cbor")]
mod cbor;
#[cfg(feature = "codec-cbor")]
#[allow(deprecated)]
pub use cbor::Cbor;
#[cfg(feature = "default-codec-cbor")]
#[doc(no_inline)]
#[allow(deprecated)]
pub use cbor::Cbor as Default;

#[cfg(feature = "codec-ciborium")]
mod ciborium;
#[cfg(feature = "codec-ciborium")]
pub use self::ciborium::Ciborium;
#[cfg(feature = "default-codec-ciborium")]
#[doc(no_inline)]
pub use self::ciborium::Ciborium as Default;

#[cfg(feature = "codec-json")]
mod json;
#[cfg(feature = "codec-json")]
pub use json::Json;
#[cfg(feature = "default-codec-json")]
#[doc(no_inline)]
pub use json::Json as Default;

#[cfg(feature = "codec-message-pack")]
mod message_pack;
#[cfg(feature = "codec-message-pack")]
pub use message_pack::MessagePack;
#[cfg(feature = "default-codec-message-pack")]
#[doc(no_inline)]
pub use message_pack::MessagePack as Default;

/// Default codec is not set and cannot be used.
///
/// Set one of the crate features `default-codec-*` to define the default codec.
///
/// This will cause a compile error when you attempt to use it.
#[cfg(not(feature = "default-codec-set"))]
pub struct Default;
