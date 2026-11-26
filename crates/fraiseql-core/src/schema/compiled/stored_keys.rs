//! The stored JSONB keys of every declared field, resolved once when the schema is built.
//!
//! A projector reads a field from one of two maps: the stored document, keyed
//! `snake_case`, or the output of `jsonb_build_object`, keyed by the camelCase response
//! name. So each field has a key and a fallback, the pair `stored_key_candidates`
//! (`runtime::projection`) builds on every call. [`build_indexes`] builds the same pair once
//! per field, keyed by type name and field name, so a read borrows both names and
//! allocates nothing.
//!
//! The key is resolved with the schema's own `naming_acronyms`. The process-wide set the
//! runtime's `to_snake_case` reads is installed at boot after the schema is built, so
//! resolving against it here would use the built-in defaults.
//!
//! The table is keyed by name, never by position. A copy of a built schema whose types
//! are filtered keeps entries for types it no longer holds, which no read reaches, and
//! cannot resolve a name to another field's keys.
//!
//! [`build_indexes`]: CompiledSchema::build_indexes

use std::{collections::HashMap, sync::Arc};

use fraiseql_db::utils::{Acronyms, to_snake_case_with};

use super::CompiledSchema;
use crate::{schema::TypeDefinition, utils::casing::to_camel_case};

/// The keys a stored or projected row holds a field under.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredKeys {
    stored:   String,
    fallback: Option<String>,
}

impl StoredKeys {
    /// The pair for `field_name`: its `snake_case` key under `acronyms`, and its camelCase
    /// spelling when that differs.
    fn resolve(field_name: &str, acronyms: &Acronyms) -> Self {
        let stored = to_snake_case_with(field_name, acronyms);
        let camel = to_camel_case(field_name);
        let fallback = (camel != stored).then_some(camel);
        Self { stored, fallback }
    }

    /// The `snake_case` key a stored document holds the field under.
    #[must_use]
    pub fn stored(&self) -> &str {
        &self.stored
    }

    /// The camelCase key `jsonb_build_object` output holds it under, when it differs.
    #[must_use]
    pub fn fallback(&self) -> Option<&str> {
        self.fallback.as_deref()
    }
}

/// Every declared field's [`StoredKeys`], by type name, then field name.
///
/// Shared, not copied, by a clone: a role-gated or introspection view clones the schema
/// per request, and a name-keyed table is as right for the filtered copy as for the
/// original.
#[derive(Debug, Clone, Default)]
pub struct StoredKeyTable(Arc<HashMap<String, HashMap<String, StoredKeys>>>);

impl StoredKeyTable {
    /// The table for `types`, resolved with `acronyms` added to the built-in defaults.
    pub(crate) fn build(types: &[TypeDefinition], acronyms: &[String]) -> Self {
        let acronyms = Acronyms::with_extra(acronyms);
        let table = types
            .iter()
            .map(|type_def| {
                let fields = type_def
                    .fields
                    .iter()
                    .map(|field| {
                        let name = field.name.as_str();
                        (name.to_string(), StoredKeys::resolve(name, &acronyms))
                    })
                    .collect();
                (type_def.name.to_string(), fields)
            })
            .collect();
        Self(Arc::new(table))
    }
}

impl CompiledSchema {
    /// The stored keys of `type_name`'s declared field `field_name`, resolved when the
    /// schema was built. Borrows both names; allocates nothing.
    ///
    /// `None` for a field the type does not declare, and for a schema whose indexes were
    /// never built (a bare `serde_json::from_str`, a hand-built fixture). A caller then
    /// resolves the name itself, as it did before this table existed.
    #[must_use]
    pub fn stored_keys(&self, type_name: &str, field_name: &str) -> Option<&StoredKeys> {
        self.derived.stored_keys.0.get(type_name)?.get(field_name)
    }
}

#[cfg(test)]
#[path = "stored_keys_tests.rs"]
mod tests;
