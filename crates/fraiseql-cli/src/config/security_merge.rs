//! Merging a schema document's `security` block with the TOML file compiled beside it.
//!
//! Two workflows combine the two sources: an SDK `schema.json` compiled beside a project
//! `fraiseql.toml`, and a TOML schema compiled with `--types`. Both lower the TOML's
//! security settings into the compiled `security` section, then hand it to [`reconcile`]
//! together with the document's own block. The rule, per top-level key of the section:
//!
//! - set by both sources: the two values must agree, or the compile is refused naming the key. The
//!   comparison is per whole section, after both values are read through the compiled
//!   [`fraiseql_core::schema::SecurityConfig`], so a section the TOML declares partially still owns
//!   its defaults.
//! - set only by the document: the document's value is kept. Several keys (authorization rules, a
//!   default policy) have no project-config spelling.
//! - not in the document: the TOML's lowered value, defaults included.
//!
//! "Set by the TOML" means written in the file, not "different from its default": a
//! value-against-default test would let a document override an explicit TOML value that
//! happens to equal the default.

use std::collections::BTreeSet;

use anyhow::{Result, bail};
use serde_json::{Map, Value};

/// Keys of the compiled `security` section that a TOML security table spells by name.
const NAMED_KEYS: &[&str] = &[
    "api_keys",
    "cost_budget",
    "default_policy",
    "default_role",
    "enterprise",
    "error_sanitization",
    "field_auth",
    "multi_tenant",
    "persisted_queries_only",
    "pkce",
    "policies",
    "rate_limiting",
    "rls",
    "role_definitions",
    "rules",
    "service_accounts",
    "state_encryption",
    "token_revocation",
    "trusted_documents",
];

/// Compiled `security` keys a project `fraiseql.toml` sets: its `[fraiseql.security]`
/// keys by name, and `tenancy` for `[fraiseql.tenancy]`.
#[must_use]
pub fn declared_in_project_config(table: &toml::Table) -> BTreeSet<String> {
    let fraiseql = table.get("fraiseql").and_then(toml::Value::as_table);
    let security = fraiseql.and_then(|f| f.get("security")).and_then(toml::Value::as_table);
    declared_keys(security, fraiseql.is_some_and(|f| f.contains_key("tenancy")))
}

/// Compiled `security` keys a TOML schema sets: its `[security]` keys by name, and
/// `tenancy` for `[tenancy]`.
#[must_use]
pub fn declared_in_toml_schema(table: &toml::Table) -> BTreeSet<String> {
    declared_keys(
        table.get("security").and_then(toml::Value::as_table),
        table.contains_key("tenancy"),
    )
}

fn declared_keys(security: Option<&toml::Table>, tenancy: bool) -> BTreeSet<String> {
    let mut declared: BTreeSet<String> = security
        .into_iter()
        .flat_map(toml::Table::keys)
        .filter(|key| NAMED_KEYS.contains(&key.as_str()))
        .cloned()
        .collect();
    if tenancy {
        declared.insert("tenancy".to_owned());
    }
    declared
}

/// Merge the TOML's lowered `security` section with the schema document's block.
///
/// `toml_label` names the TOML file in a refusal; `declared` holds the keys the TOML
/// sets (see [`declared_in_project_config`] and [`declared_in_toml_schema`]). Without a
/// document block, `lowered` is returned unchanged.
///
/// # Errors
///
/// Returns an error when the document's block is not an object, or when a key set by
/// both sources has different values.
pub fn reconcile(
    toml_label: &str,
    lowered: Value,
    declared: &BTreeSet<String>,
    document: Option<Value>,
) -> Result<Value> {
    let Some(document) = document else {
        return Ok(lowered);
    };
    let Value::Object(document) = document else {
        bail!("the schema document's `security` must be an object, found {document}");
    };
    let mut merged = match lowered {
        Value::Object(map) => map,
        _ => Map::new(),
    };
    for (key, from_document) in document {
        if !declared.contains(&key) {
            merged.insert(key, from_document);
            continue;
        }
        let from_toml = merged.get(&key).cloned().unwrap_or(Value::Null);
        if normalized(&key, &from_toml) != normalized(&key, &from_document) {
            bail!(
                "{toml_label} and the schema document both set security `{key}`, with \
                 different values.\n  {toml_label}: {from_toml}\n  schema document: \
                 {from_document}\nSet it in one place, or make the two agree."
            );
        }
    }
    Ok(Value::Object(merged))
}

/// `value` as the compiled section reads it under `key`, defaults filled; `Null` for a
/// value equal to the section's default. A value the section cannot read is compared as
/// written, so it can only disagree.
fn normalized(key: &str, value: &Value) -> Value {
    let mut single = Map::new();
    single.insert(key.to_owned(), value.clone());
    serde_json::from_value::<fraiseql_core::schema::SecurityConfig>(Value::Object(single))
        .and_then(serde_json::to_value)
        .map_or_else(|_| value.clone(), |read| read.get(key).cloned().unwrap_or(Value::Null))
}

#[cfg(test)]
mod tests;
