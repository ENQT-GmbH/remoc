//! Serializes and deserializes a [BTreeMap]`<K, V>` as a [Vec]`<(K, V)>`.
//!
//! This is necessary for the [JSON codec](crate::codec::Json) since it does not
//! support non-string keys on dictionaries.
//!
//! Use by applying the attribute `#[serde(with="remoc::codec::map::btreemap")]` on a field.
//!
//! # Example
//!
//! The following example shows how to apply the attribute to a field in a struct.
//! Since the keys are of non-string type `Vec<u8>` [serde_json] would not be able to
//! serialize this without the attribute.
//!
#![cfg_attr(
    feature = "codec-json",
    doc = r##"
```
use std::collections::BTreeMap;
use serde::{Serialize, Deserialize};

#[derive(Serialize, Deserialize, Default)]
pub struct TestStruct {
    #[serde(with = "remoc::codec::map::btreemap")]
    btreemap: BTreeMap<Vec<u8>, String>,
}

serde_json::to_string(&TestStruct::default()).unwrap();
```
"##
)]

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::collections::BTreeMap;

/// Serialization function.
pub fn serialize<S, K, V>(h: &BTreeMap<K, V>, serializer: S) -> Result<S::Ok, S::Error>
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
pub fn deserialize<'de, D, K, V>(deserializer: D) -> Result<BTreeMap<K, V>, D::Error>
where
    D: Deserializer<'de>,
    K: Serialize + Deserialize<'de> + Ord,
    V: Serialize + Deserialize<'de>,
{
    let v: Vec<(K, V)> = Vec::deserialize(deserializer)?;
    let mut h = BTreeMap::new();
    for (key, value) in v {
        h.insert(key, value);
    }
    Ok(h)
}
