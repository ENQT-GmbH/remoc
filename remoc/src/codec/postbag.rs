use serde::{Deserialize, Serialize};

use super::{Codec, DeserializationError, SerializationError};

/// [Postbag codec](postbag) with full forward and backward compatibility.
///
/// Postbag is a high-performance binary codec that provides efficient data encoding
/// with configurable levels of forward and backward compatibility. This codec uses the [`Full`](postbag::Full)
/// configuration which provides maximum compatibility and schema evolution capabilities.
///
/// ## Key Features
///
/// - **Full fidelity of Rust type system**: Supports all serde-compatible types including
///   structs, enums, tuples, arrays, maps, and all primitive types
/// - **Efficient binary format**: Uses variable-length encoding (varint) for integers,
///   compact representations for common types, and minimal overhead
/// - **Full forward/backward compatibility**: Fields and enum variants can be reordered,
///   added, or removed safely
///
/// ## Forward and Backward Compatibility
///
/// The `Full` configuration provides comprehensive schema evolution capabilities:
///
/// - **Field reordering**: Struct fields can be reordered without breaking compatibility
/// - **Field addition**: New fields can be added to structs at any position
/// - **Field removal**: Existing fields can be removed without affecting deserialization
/// - **Enum variant evolution**: Enum variants can be added, removed, or reordered
/// - **Schema evolution**: Safe evolution of data structures over time
///
/// ## Numerical Identifier Encoding
///
/// Fields named `_0` through `_59` are encoded with just a single byte instead of the full
/// string identifier. Use `#[serde(rename = "...")]` to specify numerical IDs:
///
/// ```rust
/// use serde::{Serialize, Deserialize};
///
/// #[derive(Serialize, Deserialize)]
/// struct CompactData {
///     #[serde(rename = "_3")]
///     my_field: u32,
///     #[serde(rename = "_15")]
///     another_field: String,
///     // Regular field names work normally
///     normal_field: bool,
/// }
/// ```
///
/// This serializes and deserializes *with* field identifiers for maximum compatibility.
#[cfg_attr(docsrs, doc(cfg(feature = "codec-postbag")))]
#[derive(Clone, Serialize, Deserialize)]
pub struct Postbag;

impl Codec for Postbag {
    fn serialize<Writer, Item>(writer: Writer, item: &Item) -> Result<(), super::SerializationError>
    where
        Writer: std::io::Write,
        Item: serde::Serialize,
    {
        postbag::serialize::<postbag::Full, _, _>(item, writer).map_err(SerializationError::new)?;
        Ok(())
    }

    fn deserialize<Reader, Item>(reader: Reader) -> Result<Item, super::DeserializationError>
    where
        Reader: std::io::Read,
        Item: serde::de::DeserializeOwned,
    {
        let value = postbag::deserialize::<postbag::Full, _, _>(reader).map_err(DeserializationError::new)?;
        Ok(value)
    }
}

/// [Postbag slim codec](postbag) for compact, high-performance encoding.
///
/// The [`Slim`](postbag::Slim) configuration prioritizes performance and compact size over compatibility.
/// This codec provides efficient binary encoding but with limited schema evolution
/// capabilities compared to the full `Postbag` codec.
///
/// ## Key Features
///
/// - **Compact encoding**: Smaller serialized data size compared to `Full` configuration
/// - **Fast processing**: No string lookups during serialization/deserialization
/// - **High performance**: Optimized for speed and minimal overhead
///
/// ## Schema Evolution Limitations
///
/// The `Slim` configuration has limited schema evolution capabilities. **Fields and enum
/// variants must maintain their order** for compatibility.
///
/// ### Supported Changes
///
/// - **Adding fields**: New fields can be added to the **end** of structs only
/// - **Removing fields**: Fields can be removed from the **end** of structs only
/// - **Adding enum variants**: New variants can be added at the **end** of enums only
/// - **Removing enum variants**: Variants can be removed from the **end** of enums only
///
/// ### Important Compatibility Notes
///
/// - Fields and enum variants **cannot be reordered**
/// - Fields and enum variants **cannot be added or removed from the middle**
/// - Use serde defaults (`#[serde(default)]`) for new fields to ensure backward compatibility
/// - Always add new fields at the end of struct definitions
/// - Always add new enum variants at the end of enum definitions
///
/// ## When to Use
///
/// Choose `PostbagSlim` when:
/// - Maximum performance and minimal serialized size are priorities
/// - Schema changes are infrequent or controlled
/// - Data structures have stable field ordering
///
/// Choose the full `Postbag` codec when schema flexibility is more important than
/// the slight performance overhead.
///
/// This serializes and deserializes *without* field identifiers for maximum efficiency.
#[cfg_attr(docsrs, doc(cfg(feature = "codec-postbag")))]
#[derive(Clone, Serialize, Deserialize)]
pub struct PostbagSlim;

impl Codec for PostbagSlim {
    fn serialize<Writer, Item>(writer: Writer, item: &Item) -> Result<(), super::SerializationError>
    where
        Writer: std::io::Write,
        Item: serde::Serialize,
    {
        postbag::serialize::<postbag::Slim, _, _>(item, writer).map_err(SerializationError::new)?;
        Ok(())
    }

    fn deserialize<Reader, Item>(reader: Reader) -> Result<Item, super::DeserializationError>
    where
        Reader: std::io::Read,
        Item: serde::de::DeserializeOwned,
    {
        let value = postbag::deserialize::<postbag::Slim, _, _>(reader).map_err(DeserializationError::new)?;
        Ok(value)
    }
}
