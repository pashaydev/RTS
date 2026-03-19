//! Binary codec — MessagePack serialization for wire protocol.
//!
//! MessagePack is a self-describing binary format that supports all serde
//! attributes (internally tagged enums, skip_serializing_if, etc.) while
//! being ~2-4x smaller than JSON.

use serde::{Deserialize, Serialize};

/// Serialize a message to MessagePack binary (named/map format).
///
/// Uses `to_vec_named` (maps with field names) instead of `to_vec` (compact/arrays)
/// because the compact format breaks serde attributes like `#[serde(tag = "...")]`
/// and `#[serde(skip_serializing_if)]` on nested structures.
pub fn encode<T: Serialize>(msg: &T) -> Result<Vec<u8>, rmp_serde::encode::Error> {
    rmp_serde::to_vec_named(msg)
}

/// Deserialize a message from MessagePack binary.
pub fn decode<'a, T: Deserialize<'a>>(bytes: &'a [u8]) -> Result<T, rmp_serde::decode::Error> {
    rmp_serde::from_slice(bytes)
}

/// Serialize to JSON (for debug display only).
pub fn to_debug_json<T: Serialize>(msg: &T) -> String {
    serde_json::to_string(msg).unwrap_or_else(|_| "<serialize error>".to_string())
}
