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

/// The content hash of a compiled schema, as a hex string.
///
/// This is the **one** place the digest is defined. The CLI embeds it, `from_json`
/// re-derives it to verify, [`CompiledSchema::content_hash`] uses it as the cache
/// version, and the integrity test drives it — because three hand-copies of
/// "canonicalize, pretty-print, SHA-256, take 16 bytes" is three chances to
/// disagree, and they did: the integrity test hashed the *uncanonicalized* value
/// and passed only because `serde_json::Map` happened to be a sorted `BTreeMap`
/// in the build it ran in (#899).
///
/// `value` must not contain the `_content_hash` key: a schema cannot hash its own
/// hash.
#[must_use]
pub fn content_hash_of(value: &serde_json::Value) -> String {
    let canonical = canonicalize_json(value);
    // `to_string_pretty` on a canonicalized value cannot fail — every node is a
    // plain JSON value with no non-string map keys.
    let rendered = serde_json::to_string_pretty(&canonical).unwrap_or_default();
    let digest = Sha256::digest(rendered.as_bytes());
    hex::encode(&digest[..16]) // 32 hex chars — sufficient collision resistance
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
        // Typed deserialization goes through `serde_path_to_error` so a refused
        // schema names the offending section ("security.rate_limiting.…"), not
        // just a line/column into a machine-formatted file — the #778 property,
        // kept now that malformed sections are refused at load (#977).
        let path_err = |e: serde_path_to_error::Error<serde_json::Error>| FraiseQLError::Parse {
            message:  format!("Schema JSON parse error at `{}`: {}", e.path(), e.inner()),
            location: e.path().to_string(),
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
            let de = &mut serde_json::Deserializer::from_str(json);
            let mut schema: Self = serde_path_to_error::deserialize(de).map_err(path_err)?;
            schema.build_indexes();
            schema.finish_load()?;
            return Ok(schema);
        };

        // Canonicalize and serialize deterministically (sorted keys at all levels)
        let computed_hash = content_hash_of(&value);

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

        // Now deserialize the schema from the remaining JSON (the parsed value
        // with `_content_hash` already removed).
        let mut schema: Self = serde_path_to_error::deserialize(value).map_err(path_err)?;
        schema.build_indexes();
        schema.finish_load()?;
        Ok(schema)
    }

    /// Normalization every load must perform, and the checks that must hold after it.
    ///
    /// Lives here rather than in `build_indexes` because it can fail, and because
    /// both `from_json` branches must run it — the hashed and the unhashed one. A
    /// normalization step performed on one load path and not its sibling is the
    /// shape that produced #748 and #812 in other subsystems.
    ///
    /// Today it refuses a schema that declares a name twice (#1265), lowers each
    /// type's `requires_role` onto the operations that return it (#677) and refuses a
    /// schema whose role declarations the runtime cannot honour, then refuses one whose
    /// type-level and query-level scoping declarations contradict each other (#1142),
    /// one whose subscription row-visibility policy the delivery path cannot honour
    /// (#596/#1265), one whose subscription filter names an argument it does not
    /// declare (#1262), and one declaring a relationship no embed can follow (#1266).
    ///
    /// # Errors
    ///
    /// Returns [`FraiseQLError::Validation`] when a role-gated type is declared in a
    /// shape no execution path can enforce — see
    /// [`CompiledSchema::type_role_violations`] — when a type and its backing query
    /// scope the same column from different sources, see
    /// [`CompiledSchema::type_inject_violations`], or when a relationship names a
    /// target, a join column or a list query the embed executor cannot resolve, see
    /// [`CompiledSchema::relationship_violations`].
    fn finish_load(&mut self) -> std::result::Result<(), FraiseQLError> {
        // First: a name declared twice makes every later check ambiguous. `build_indexes`
        // keys by name, so the second definition silently shadows the first and the
        // schema behaves as something nobody wrote (#1265).
        let violations = self.duplicate_name_violations();
        if !violations.is_empty() {
            return Err(FraiseQLError::Validation {
                message: format!(
                    "schema declares a name twice:\n  - {}",
                    violations.join("\n  - ")
                ),
                path:    Some("schema.names".to_string()),
            });
        }
        self.propagate_type_roles();
        let violations = self.type_role_violations();
        if !violations.is_empty() {
            return Err(FraiseQLError::Validation {
                message: format!(
                    "type-level `requires_role` cannot be enforced as declared:\n  - {}",
                    violations.join("\n  - ")
                ),
                path:    Some("security.requires_role".to_string()),
            });
        }
        let violations = self.type_inject_violations();
        if !violations.is_empty() {
            return Err(FraiseQLError::Validation {
                message: format!(
                    "type-level `inject_params` cannot be enforced as declared:\n  - {}",
                    violations.join("\n  - ")
                ),
                path:    Some("security.inject_params".to_string()),
            });
        }
        let violations = self.subscription_policy_violations();
        if !violations.is_empty() {
            return Err(FraiseQLError::Validation {
                message: format!(
                    "subscription policies cannot be applied as declared:\n  - {}",
                    violations.join("\n  - ")
                ),
                path:    Some("types.subscription_policy".to_string()),
            });
        }
        let violations = self.subscription_filter_violations();
        if !violations.is_empty() {
            return Err(FraiseQLError::Validation {
                message: format!(
                    "subscription filters cannot be applied as declared:\n  - {}",
                    violations.join("\n  - ")
                ),
                path:    Some("subscriptions.filter".to_string()),
            });
        }
        let violations = self.relationship_violations();
        if !violations.is_empty() {
            return Err(FraiseQLError::Validation {
                message: format!(
                    "relationships cannot be followed as declared:\n  - {}",
                    violations.join("\n  - ")
                ),
                path:    Some("types.relationships".to_string()),
            });
        }
        Ok(())
    }

    /// Types whose declared subscription row-visibility policy the delivery path cannot
    /// honour (#596).
    ///
    /// This check lived in `CompiledSchema::validate()`, which had **no caller outside
    /// tests** — so #596's own comment calling it "a load-time error, not a silent
    /// deliver-all at subscribe time" described something that never ran, and
    /// `SubscriptionPolicy::validate` had exactly one caller: the unreachable one
    /// (#1265).
    ///
    /// It matters precisely here, because `subscription_policy` has no authoring
    /// producer at all — the CLI hardcodes `None` at every construction site, the
    /// intermediate schema has no field for it and no SDK emits one. The only way a
    /// policy reaches a deployment is a hand-written compiled schema, which is exactly
    /// the input this path accepts.
    fn subscription_policy_violations(&self) -> Vec<String> {
        self.types
            .iter()
            .filter_map(|type_def| {
                type_def
                    .subscription_policy
                    .as_ref()
                    .and_then(|policy| policy.validate().err())
                    .map(|e| format!("Type '{}': {e}", type_def.name))
            })
            .collect()
    }

    /// Types, queries or mutations declared under a name already taken (#1265).
    ///
    /// `build_indexes` keys each collection by name, so a duplicate does not conflict —
    /// it *shadows*, and the schema serves whichever definition indexed last while the
    /// document plainly contains both. `fraiseql compile` refuses to emit one
    /// (`SchemaValidator::validate` checks the intermediate schema), which leaves the
    /// hand-edited artifact.
    ///
    /// ⚠ `CompiledSchema::validate()` also checked that every query and mutation return
    /// type resolved, and that check is **not** carried over, because it was wrong: it
    /// resolved against `self.types` plus ten builtin scalar names, while the schema
    /// holds `enums`, `interfaces` and `unions` in separate collections. Measured before
    /// its removal, it reported `Query 'orderStatus' references undefined type
    /// 'OrderStatus'` for a valid enum-returning query — so moving it onto this path
    /// would have refused, at boot, every schema with one. Nobody noticed in the years
    /// it existed, because nothing ran it. `SchemaValidator::validate` covers return
    /// types on the compile path, correctly. See
    /// `a_query_returning_an_enum_or_union_still_loads`.
    fn duplicate_name_violations(&self) -> Vec<String> {
        fn duplicates<'a>(kind: &str, names: impl Iterator<Item = &'a str>, out: &mut Vec<String>) {
            let mut seen: std::collections::HashSet<&str> = std::collections::HashSet::new();
            for name in names {
                if !seen.insert(name) {
                    out.push(format!("Duplicate {kind} name: {name}"));
                }
            }
        }

        let mut violations = Vec::new();
        duplicates("type", self.types.iter().map(|t| t.name.as_str()), &mut violations);
        duplicates("query", self.queries.iter().map(|q| q.name.as_str()), &mut violations);
        duplicates("mutation", self.mutations.iter().map(|m| m.name.as_str()), &mut violations);
        violations
    }

    /// Subscriptions whose filter names an argument they do not declare (#1262).
    ///
    /// `fraiseql compile` refuses to emit such a document, which leaves the hand-edited
    /// artifact — the case a compile-time check cannot reach, and the reason this is
    /// checked again on the load path every entry point shares.
    fn subscription_filter_violations(&self) -> Vec<String> {
        self.subscriptions
            .iter()
            .filter_map(crate::schema::SubscriptionDefinition::filter_violation)
            .collect()
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
        let json = self.to_json().expect("CompiledSchema always serialises — BUG if this fails");
        let value: serde_json::Value =
            serde_json::from_str(&json).expect("just serialised — always valid JSON");
        content_hash_of(&value)
    }
}
