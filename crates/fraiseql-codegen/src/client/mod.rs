//! Consumer-side client code generation from a [`CompiledSchema`].
//!
//! Given a compiled schema, the generators here emit typed clients that
//! *callers* of a FraiseQL API use to query and mutate it — interfaces for every
//! type, typed query/mutation functions, relationship metadata, and a tiny
//! `fetch`-based runtime client.
//!
//! Each generated file is stamped with a hash of the schema it was generated
//! from (see [`schema_hash`]). Consumers can recompute the live schema's hash in
//! CI and fail the build when the generated client drifts out of date.

use fraiseql_core::schema::CompiledSchema;
use sha2::{Digest, Sha256};

use crate::{FraiseQLError, Result};

pub mod typescript;

/// Compute the canonical schema hash used to stamp generated files.
///
/// The hash is `sha256` over a **canonical** JSON encoding of the schema:
/// object keys are sorted recursively so the digest is independent of field
/// declaration order, serializer settings, or `serde` feature flags. Array order
/// is preserved (it is semantically meaningful in the schema).
///
/// This canonicalization is a **stable consumer-facing contract** — a CI check
/// compares this value against the `schema-hash` stamp in generated files. Do not
/// change the canonicalization without treating it as a breaking change.
///
/// # Errors
///
/// Returns [`FraiseQLError::Internal`] if the schema cannot be serialized to JSON
/// (not expected for a well-formed [`CompiledSchema`]).
pub fn schema_hash(schema: &CompiledSchema) -> Result<String> {
    let value = serde_json::to_value(schema).map_err(|e| {
        FraiseQLError::internal(format!("failed to serialize schema for hashing: {e}"))
    })?;
    let canonical = canonicalize(&value);
    let encoded = serde_json::to_string(&canonical)
        .map_err(|e| FraiseQLError::internal(format!("failed to encode canonical schema: {e}")))?;

    let mut hasher = Sha256::new();
    hasher.update(encoded.as_bytes());
    Ok(hex::encode(hasher.finalize()))
}

/// Recursively rebuild a JSON value with object keys sorted.
///
/// Building each object from key-sorted entries makes the output deterministic
/// whether `serde_json`'s `Map` is a `BTreeMap` or an `IndexMap` (`preserve_order`).
fn canonicalize(value: &serde_json::Value) -> serde_json::Value {
    match value {
        serde_json::Value::Object(map) => {
            let mut entries: Vec<(&String, &serde_json::Value)> = map.iter().collect();
            entries.sort_by(|a, b| a.0.cmp(b.0));
            let sorted = entries.into_iter().map(|(k, v)| (k.clone(), canonicalize(v))).collect();
            serde_json::Value::Object(sorted)
        },
        serde_json::Value::Array(items) => {
            serde_json::Value::Array(items.iter().map(canonicalize).collect())
        },
        other => other.clone(),
    }
}

#[cfg(test)]
mod tests;
