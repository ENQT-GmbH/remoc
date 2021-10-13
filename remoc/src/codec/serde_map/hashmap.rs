//! Serializes and deserializes a [HashMap]`<K, V>` as a [Vec]`<(K, V)>`.
//!
//! This is necessary for the [JsonCodec](crate::codec::JsonCodec) since it does not
//! support non-string keys on dictionaries.
//!
//! Use by applying the attribute `#[serde(with="remoc::codec::serde_map::hashmap")]` on a field.

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::{collections::HashMap, hash::Hash};

/// Serialization function.
pub fn serialize<S, K, V>(h: &HashMap<K, V>, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
    K: Serialize + Clone,
    V: Serialize + Clone,
{
    let mut v: Vec<(K, V)> = Vec::new();
    for (key, value) in h {
        v.push((key.clone(), value.clone()));
    }
    v.serialize(serializer)
}

/// Deserialization function.
pub fn deserialize<'de, D, K, V>(deserializer: D) -> Result<HashMap<K, V>, D::Error>
where
    D: Deserializer<'de>,
    K: Serialize + Deserialize<'de> + Eq + Hash,
    V: Serialize + Deserialize<'de>,
{
    let v: Vec<(K, V)> = Vec::deserialize(deserializer)?;
    let mut h = HashMap::new();
    for (key, value) in v {
        h.insert(key, value);
    }
    Ok(h)
}
