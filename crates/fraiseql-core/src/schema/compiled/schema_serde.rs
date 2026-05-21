//! Serialization, deserialization, and integrity checking for [`CompiledSchema`].

use sha2::{Digest, Sha256};
use tracing::{info, warn};

use super::schema::CompiledSchema;
use crate::error::FraiseQLError;

/// Recursively sort all JSON object keys to produce a canonical representation.
///
/// This guarantees deterministic serialization regardless of `HashMap` iteration
/// order or `serde_json` feature flags (`preserve_order`). Used by both the CLI
/// (hash embed) and `from_json` (hash verify) to ensure round-trip consistency.
///
/// # Example
///
/// ```
/// use fraiseql_core::schema::canonicalize_json;
///
/// let v: serde_json::Value = serde_json::from_str(r#"{"b":1,"a":2}"#).unwrap();
/// let c = canonicalize_json(&v);
/// assert_eq!(serde_json::to_string(&c).unwrap(), r#"{"a":2,"b":1}"#);
/// ```
#[must_use]
pub fn canonicalize_json(value: &serde_json::Value) -> serde_json::Value {
    match value {
        serde_json::Value::Object(map) => {
            let mut sorted = serde_json::Map::new();
            let mut keys: Vec<&String> = map.keys().collect();
            keys.sort();
            for key in keys {
                sorted.insert(key.clone(), canonicalize_json(&map[key]));
            }
            serde_json::Value::Object(sorted)
        },
        serde_json::Value::Array(arr) => {
            serde_json::Value::Array(arr.iter().map(canonicalize_json).collect())
        },
        other => other.clone(),
    }
}

impl CompiledSchema {
    /// Deserialize from JSON string.
    ///
    /// This is the primary way to create a schema from any authoring language.
    /// The authoring language emits `schema.json`; `fraiseql-cli compile` produces
    /// `schema.compiled.json`; Rust deserializes and owns the result.
    ///
    /// # Integrity Checking
    ///
    /// `fraiseql-cli compile` embeds a `_content_hash` field (SHA-256 of the compiled JSON
    /// body, first 16 bytes as lowercase hex) in the compiled output. This function
    /// extracts that field, recomputes the hash over the remaining JSON, and compares.
    ///
    /// - `strict_integrity = true`: missing or mismatched hash returns `Err`.
    /// - `strict_integrity = false`: missing hash logs a warning; mismatch logs a warning but
    ///   proceeds (backwards compatibility for schemas compiled without `_content_hash`).
    ///
    /// # Errors
    ///
    /// Returns error if JSON is malformed or doesn't match schema structure.
    ///
    /// # Example
    ///
    /// ```
    /// use fraiseql_core::schema::CompiledSchema;
    ///
    /// let json = r#"{"types": [], "queries": [], "mutations": [], "subscriptions": []}"#;
    /// let schema = CompiledSchema::from_json(json, false).unwrap();
    /// ```
    pub fn from_json(
        json: &str,
        strict_integrity: bool,
    ) -> std::result::Result<Self, FraiseQLError> {
        let serde_err = |e: serde_json::Error| FraiseQLError::Parse {
            message:  format!("Schema JSON parse error: {e}"),
            location: String::new(),
        };

        let mut value: serde_json::Value = serde_json::from_str(json).map_err(serde_err)?;

        let obj = value.as_object_mut().ok_or_else(|| FraiseQLError::Validation {
            message: "Schema JSON must be an object".to_string(),
            path:    None,
        })?;

        // Extract and remove _content_hash
        let expected_hash = if let Some(hash_val) = obj.remove("_content_hash") {
            if let Some(hash_str) = hash_val.as_str() {
                Some(hash_str.to_string())
            } else {
                return Err(FraiseQLError::Validation {
                    message: "_content_hash must be a string".to_string(),
                    path:    None,
                });
            }
        } else if strict_integrity {
            return Err(FraiseQLError::Validation {
                message: "Schema integrity check failed: missing _content_hash field. Enable strict_schema_integrity=false for backwards compatibility.".to_string(),
                path: None,
            });
        } else {
            warn!(
                "Schema integrity check skipped: no _content_hash field present. Consider recompiling with a newer CLI for integrity verification."
            );
            // No hash, parse directly from original
            let mut schema: Self = serde_json::from_str(json).map_err(serde_err)?;
            schema.build_indexes();
            return Ok(schema);
        };

        // Canonicalize and serialize deterministically (sorted keys at all levels)
        let canonical = canonicalize_json(&value);
        let remaining_json = serde_json::to_string_pretty(&canonical).map_err(serde_err)?;
        let computed_digest = Sha256::digest(remaining_json.as_bytes());
        let computed_hash = hex::encode(&computed_digest[..16]);

        if let Some(expected) = expected_hash {
            if expected != computed_hash {
                if strict_integrity {
                    return Err(FraiseQLError::Validation {
                        message: format!(
                            "Schema integrity check failed: hash mismatch (expected {expected}, got {computed_hash})"
                        ),
                        path:    None,
                    });
                }
                warn!(
                    "Schema integrity check: hash mismatch (expected {expected}, got {computed_hash}). Proceeding because strict_integrity is disabled."
                );
            } else {
                info!("Schema integrity verified: hash matches");
            }
        }

        // Now deserialize the schema from the remaining JSON
        let mut schema: Self = serde_json::from_str(&remaining_json).map_err(serde_err)?;
        schema.build_indexes();
        Ok(schema)
    }

    /// Serialize to JSON string.
    ///
    /// # Errors
    ///
    /// Returns error if serialization fails (should not happen for valid schema).
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string(self)
    }

    /// Serialize to pretty JSON string (for debugging/config files).
    ///
    /// # Errors
    ///
    /// Returns error if serialization fails.
    pub fn to_json_pretty(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(self)
    }

    /// Returns a 32-character hex SHA-256 content hash of this schema's canonical JSON.
    ///
    /// Use as `schema_version` when constructing `CachedDatabaseAdapter` to guarantee
    /// cache invalidation on any schema change, regardless of whether the package
    /// version was bumped.
    ///
    /// Two schemas that differ by even one field will produce different hashes.
    /// The same schema serialised twice always produces the same hash (stable).
    ///
    /// # Panics
    ///
    /// Does not panic — `CompiledSchema` always serialises to valid JSON.
    ///
    /// # Example
    ///
    /// ```
    /// use fraiseql_core::schema::CompiledSchema;
    ///
    /// let schema = CompiledSchema::default();
    /// let hash = schema.content_hash();
    /// assert_eq!(hash.len(), 32); // 16 bytes → 32 hex chars
    /// ```
    #[must_use]
    pub fn content_hash(&self) -> String {
        use sha2::{Digest, Sha256};
        let json = self.to_json().expect("CompiledSchema always serialises — BUG if this fails");
        let digest = Sha256::digest(json.as_bytes());
        hex::encode(&digest[..16]) // 32 hex chars — sufficient collision resistance
    }
}
