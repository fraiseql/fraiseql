//! Serialization, deserialization, and integrity checking for [`CompiledSchema`].

use sha2::{Digest, Sha256};
use tracing::{info, warn};

use super::schema::CompiledSchema;
use crate::error::FraiseQLError;

/// The setting namespace the server sets its own session settings in (`fraiseql.locale`,
/// `fraiseql.started_at`), closed to `[[session_variables.variables]]`.
const RESERVED_SETTING_PREFIX: &str = "fraiseql.";

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
    /// declare (#1262), one declaring `requires_scope` with no role able to grant it
    /// (ruling Y 7), and one declaring a relationship no embed can follow (#1266).
    ///
    /// # Errors
    ///
    /// Returns [`FraiseQLError::Validation`] when a role-gated type is declared in a
    /// shape no execution path can enforce — see
    /// [`CompiledSchema::type_role_violations`] — when a type and its backing query
    /// scope the same column from different sources, see
    /// [`CompiledSchema::type_inject_violations`], when a federation entity's `@key`
    /// cannot be served, see [`CompiledSchema::federation_key_problems`], or when a
    /// relationship names a target, a join column or a list query the embed executor
    /// cannot resolve, see [`CompiledSchema::relationship_violations`].
    /// Every field whose `hierarchy` names no `[hierarchies.<name>]` entry, and every
    /// fact-table filter whose `hierarchy` names none or sits on a column that is not an
    /// ltree path.
    fn hierarchy_link_violations(&self) -> Vec<String> {
        let declared = self.hierarchies_config.as_ref();
        let mut violations: Vec<String> = self
            .types
            .iter()
            .flat_map(|t| t.fields.iter().map(move |f| (t, f)))
            .filter_map(|(t, f)| {
                let name = f.hierarchy.as_deref()?;
                if declared.is_some_and(|h| h.contains_key(name)) {
                    return None;
                }
                Some(format!(
                    "field `{}.{}` links hierarchy `{name}`, which no `[hierarchies.{name}]` \
                     table declares (table + path_column)",
                    t.name, f.name
                ))
            })
            .collect();
        // A fact table's path column resolves node ids through its hierarchy too (#1498).
        for (table, metadata) in &self.fact_tables {
            for filter in &metadata.denormalized_filters {
                let Some(name) = filter.hierarchy.as_deref() else {
                    continue;
                };
                if filter.sql_type != crate::compiler::fact_table::SqlType::Ltree {
                    violations.push(format!(
                        "fact table filter `{table}.{}` links hierarchy `{name}` but is not an \
                         ltree path column (sql_type {:?})",
                        filter.name, filter.sql_type
                    ));
                } else if !declared.is_some_and(|h| h.contains_key(name)) {
                    violations.push(format!(
                        "fact table filter `{table}.{}` links hierarchy `{name}`, which no \
                         `[hierarchies.{name}]` table declares (table + path_column)",
                        filter.name
                    ));
                }
            }
        }
        violations.sort();
        violations
    }

    /// A localized argument or input field (#1513) is a `String` written as a locale map,
    /// coerced through `[locale]`: on another type, or without `[locale]`, there is nothing to
    /// coerce it with. And the names its input types are published under are the schema's.
    fn localized_input_violations(&self) -> Vec<String> {
        let mut violations = Vec::new();
        let mut check = |at: String, is_string: bool| {
            if !is_string {
                violations.push(format!(
                    "`{at}` is localized but is not a String (a localized input is a String \
                     written as a locale map)"
                ));
            } else if self.locale.is_none() {
                violations.push(format!(
                    "`{at}` is localized, but the schema declares no [locale]: add [locale] to \
                     fraiseql.toml"
                ));
            }
        };
        let operations = self
            .queries
            .iter()
            .map(|q| (q.name.as_str(), &q.arguments))
            .chain(self.mutations.iter().map(|m| (m.name.as_str(), &m.arguments)))
            .chain(self.subscriptions.iter().map(|s| (s.name.as_str(), &s.arguments)));
        for (operation, arguments) in operations {
            for argument in arguments.iter().filter(|a| a.localized) {
                check(
                    format!("{operation}({})", argument.name),
                    matches!(argument.arg_type, crate::schema::FieldType::String),
                );
            }
        }
        for input in &self.input_types {
            for field in input.fields.iter().filter(|f| f.localized) {
                check(
                    format!("{}.{}", input.name, field.name),
                    field.field_type.trim_end_matches('!') == "String",
                );
            }
        }
        if self.has_localized_inputs() {
            for name in [
                crate::schema::LOCALIZED_INPUT_TYPE,
                crate::schema::LOCALIZED_STRING_INPUT_TYPE,
            ] {
                let declared = self.types.iter().any(|t| t.name == name)
                    || self.enums.iter().any(|e| e.name == name)
                    || self.input_types.iter().any(|i| i.name == name)
                    || self.interfaces.iter().any(|i| i.name == name)
                    || self.unions.iter().any(|u| u.name == name);
                if declared {
                    violations.push(format!(
                        "`{name}` is declared, but it is the input type of every localized \
                         argument and input field: rename the type"
                    ));
                }
            }
        }
        violations
    }

    /// The uses of a localized field that have no meaning (#1513): a fact-table measure (a
    /// label is text, and a measure is a number aggregated, #1524) and a federation `@key`
    /// (#1526). A localized dimension groups by its label (#1524), and a subscription filter
    /// compares it (#1525).
    fn localized_use_violations(&self) -> Vec<String> {
        use crate::compiler::fact_table::dimension_key;
        let localized_of = |type_name: &str| -> Vec<&str> {
            self.find_type(type_name).map_or_else(Vec::new, |t| {
                t.fields.iter().filter(|f| f.localized).map(|f| f.name.as_str()).collect()
            })
        };
        let mut violations = Vec::new();
        let mut fact_tables: Vec<_> = self.fact_tables.values().collect();
        fact_tables.sort_by(|a, b| a.table_name.cmp(&b.table_name));
        for ft in fact_tables {
            let Some(type_name) = ft.type_name.as_deref() else {
                continue;
            };
            for field in localized_of(type_name) {
                let key = dimension_key(field);
                if ft.measures.iter().any(|m| dimension_key(&m.name) == key) {
                    violations.push(format!(
                        "fact table `{}` has a measure `{field}`, but `{type_name}.{field}` is \
                         localized: a measure is a number aggregated, and a label is text (group \
                         by it as a dimension instead, #1524)",
                        ft.table_name
                    ));
                }
            }
        }
        if let Some(federation) = &self.federation {
            for entity in &federation.entities {
                let localized = localized_of(&entity.name);
                let keyed = entity.key_fields.iter().flat_map(|k| {
                    k.split(|c: char| !(c.is_alphanumeric() || c == '_')).filter(|t| !t.is_empty())
                });
                for field in keyed.filter(|t| localized.contains(t)) {
                    violations.push(format!(
                        "federation entity `{}` is keyed by `{field}`, which is localized: a \
                         localized field cannot be an `@key` (#1526)",
                        entity.name
                    ));
                }
            }
        }
        violations
    }

    /// Validate `[locale]` and derive its chains; refuse a session variable that would
    /// shadow the setting the server owns (#1512).
    fn validate_locale(&mut self) -> std::result::Result<(), FraiseQLError> {
        if let Some(locale) = self.locale.as_mut() {
            locale.validate()?;
        }
        // #1513: a localized field is a `String` stored as a locale map, resolved through
        // `[locale]`'s chains; on another type, or without `[locale]`, there is nothing to
        // resolve it with.
        let mut localized = Vec::new();
        for type_def in &self.types {
            for field in type_def.fields.iter().filter(|f| f.localized) {
                if !matches!(field.field_type, crate::schema::FieldType::String) {
                    localized.push(format!(
                        "`{}.{}` is localized but is not a String (a localized field is a \
                         String stored as a locale map)",
                        type_def.name, field.name
                    ));
                } else if self.locale.is_none() {
                    localized.push(format!(
                        "`{}.{}` is localized, but the schema declares no [locale]: add \
                         [locale] to fraiseql.toml",
                        type_def.name, field.name
                    ));
                }
                // The translations sibling and its element type are names the schema
                // answers for, so nothing declared may take them.
                let sibling = format!("{}{}", field.name, crate::schema::TRANSLATIONS_SUFFIX);
                if type_def.find_field(&sibling).is_some() {
                    localized.push(format!(
                        "`{}.{sibling}` is declared, but it is the translations sibling of the \
                         localized `{}.{}`: rename the field",
                        type_def.name, type_def.name, field.name
                    ));
                }
            }
        }
        let has_localized = self.types.iter().any(|t| t.fields.iter().any(|f| f.localized));
        let localized_string = crate::schema::LOCALIZED_STRING_TYPE;
        if has_localized
            && (self.types.iter().any(|t| t.name == localized_string)
                || self.enums.iter().any(|e| e.name == localized_string)
                || self.input_types.iter().any(|i| i.name == localized_string)
                || self.interfaces.iter().any(|i| i.name == localized_string)
                || self.unions.iter().any(|u| u.name == localized_string))
        {
            localized.push(format!(
                "`{localized_string}` is declared, but it is the type of every localized \
                 field's translations sibling: rename the type"
            ));
        }
        localized.extend(self.localized_use_violations());
        localized.extend(self.localized_input_violations());
        if !localized.is_empty() {
            return Err(FraiseQLError::Validation {
                message: format!(
                    "localized fields cannot be served:\n  - {}",
                    localized.join("\n  - ")
                ),
                path:    Some("types.fields.localized".to_string()),
            });
        }
        // The `fraiseql.` setting namespace is the server's own: `fraiseql.locale` on every
        // read, `fraiseql.started_at` before a mutation. A session variable there would
        // overwrite it, from a header any caller controls. PostgreSQL resolves a setting
        // name case-insensitively, so the comparison is too.
        if let Some(mapping) = self.session_variables.variables.iter().find(|m| {
            m.name
                .get(..RESERVED_SETTING_PREFIX.len())
                .is_some_and(|prefix| prefix.eq_ignore_ascii_case(RESERVED_SETTING_PREFIX))
        }) {
            let advice = if mapping.name.eq_ignore_ascii_case(crate::schema::LOCALE_SESSION_VAR) {
                "; configure the request locale with [locale] instead"
            } else {
                ""
            };
            return Err(FraiseQLError::Validation {
                message: format!(
                    "[[session_variables.variables]] declares `{}`, in the `fraiseql.` \
                     namespace the server sets its own settings in (`fraiseql.locale`, \
                     `fraiseql.started_at`): name it in another namespace{advice}",
                    mapping.name
                ),
                path:    Some("session_variables".to_string()),
            });
        }
        Ok(())
    }

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
        // #1306: a ceiling of 0 refuses every offset page; the compiler refuses it, and a
        // hand-written artifact is held to the same rule.
        if self.validation_config.as_ref().and_then(|v| v.max_offset) == Some(0) {
            return Err(FraiseQLError::Validation {
                message: "`max_offset` of 0 would refuse every offset page; leave it unset for \
                          no ceiling"
                    .to_string(),
                path:    Some("validation_config.max_offset".to_string()),
            });
        }
        // #1512: a tag reaches SQL text, so the compiled `[locale]` is checked again here,
        // where a hand-written artifact passes too, and its chains are derived (never read).
        self.validate_locale()?;
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
        // #1396: a field's `hierarchy` names the table `descendantOfId` / `ancestorOfId`
        // resolve node ids against. One naming nothing would make those operators fail on
        // every request, so the artifact is refused here, where both compile workflows and
        // a hand-edited artifact pass.
        let violations = self.hierarchy_link_violations();
        if !violations.is_empty() {
            return Err(FraiseQLError::Validation {
                message: format!(
                    "a field links a hierarchy that is not declared:\n  - {}",
                    violations.join("\n  - ")
                ),
                path:    Some("hierarchies_config".to_string()),
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
        // #1159: a query's `orderBy.field` enum lists exactly its sort keys. Derivation never
        // overwrites a name already taken, so an author's type of the same name, or two owners
        // deriving one name, would hand a query someone else's keys: refused, naming it.
        let violations = self.order_by_violations();
        if !violations.is_empty() {
            return Err(FraiseQLError::Validation {
                message: format!(
                    "a query's sort keys cannot be published as derived:\n  - {}",
                    violations.join("\n  - ")
                ),
                path:    Some("queries.orderBy".to_string()),
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
        let violations = self.scope_violations();
        if !violations.is_empty() {
            return Err(FraiseQLError::Validation {
                message: format!(
                    "field-level `requires_scope` cannot be enforced as declared:\n  - {}",
                    violations.join("\n  - ")
                ),
                path:    Some("security.requires_scope".to_string()),
            });
        }
        let violations = self.fact_table_link_violations();
        if !violations.is_empty() {
            return Err(FraiseQLError::Validation {
                message: format!(
                    "fact tables cannot be read as their types:\n  - {}",
                    violations.join("\n  - ")
                ),
                path:    Some("fact_tables.type_name".to_string()),
            });
        }
        let violations = self.fact_table_mapping_violations();
        if !violations.is_empty() {
            return Err(FraiseQLError::Validation {
                message: format!(
                    "fact table dimensions are declared twice:\n  - {}",
                    violations.join("\n  - ")
                ),
                path:    Some("fact_tables.native_dimension_mapping".to_string()),
            });
        }
        let violations = self.fact_table_mapping_column_violations();
        if !violations.is_empty() {
            return Err(FraiseQLError::Validation {
                message: format!(
                    "fact table dimensions are mapped to undeclared columns:\n  - {}",
                    violations.join("\n  - ")
                ),
                path:    Some("fact_tables.native_dimension_mapping".to_string()),
            });
        }
        let violations = self.fact_table_path_violations();
        if !violations.is_empty() {
            return Err(FraiseQLError::Validation {
                message: format!(
                    "fact table dimension paths cannot be read:\n  - {}",
                    violations.join("\n  - ")
                ),
                path:    Some("fact_tables.dimensions.paths".to_string()),
            });
        }
        let violations = self.federation_key_violations();
        if !violations.is_empty() {
            return Err(FraiseQLError::Validation {
                message: format!(
                    "federation entities cannot be served as declared:\n  - {}",
                    violations.join("\n  - ")
                ),
                path:    Some("federation.entities".to_string()),
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
    /// Every sorting query whose derived `orderBy` item names an enum that is not exactly its
    /// sort keys, or a name another kind of type also takes (#1159). An item type the author
    /// declared is their surface, and not checked.
    fn order_by_violations(&self) -> Vec<String> {
        use crate::schema::derived_inputs::{order_by_type_names, sortable_keys};
        let is_name = |key: &str| {
            let mut chars = key.chars();
            chars.next().is_some_and(|c| c == '_' || c.is_ascii_alphabetic())
                && chars.all(|c| c == '_' || c.is_ascii_alphanumeric())
                && !matches!(key, "true" | "false" | "null")
        };
        let mut out = Vec::new();
        for query in &self.queries {
            let Some((item, field)) = order_by_type_names(self, query) else {
                continue;
            };
            let derived = self.find_input_type(&item).is_some_and(|input| {
                input.fields.iter().any(|f| f.name == "field" && f.field_type == field)
            });
            if !derived {
                continue;
            }
            let keys =
                sortable_keys(self, &query.return_type, &query.native_columns).unwrap_or_default();
            if let Some(bad) = keys.iter().find(|k| !is_name(k)) {
                out.push(format!(
                    "`{}`: the sort key `{bad}` is not a GraphQL name, so `{field}` cannot list it",
                    query.name
                ));
            }
            let listed = self
                .find_enum(&field)
                .map(|e| e.values.iter().map(|v| v.name.as_str()).collect::<Vec<_>>());
            let taken = self.find_type(&field).is_some()
                || self.find_input_type(&field).is_some()
                || self.find_interface(&field).is_some()
                || self.find_union(&field).is_some();
            if taken
                || listed.as_deref()
                    != Some(keys.iter().map(String::as_str).collect::<Vec<_>>().as_slice())
            {
                out.push(format!(
                    "`{}`: its sort keys need the enum `{field}`, and another declaration takes \
                     that name; rename the query or the other type",
                    query.name
                ));
            }
        }
        out
    }

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
