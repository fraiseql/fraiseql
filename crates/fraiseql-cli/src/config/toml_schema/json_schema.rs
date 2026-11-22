//! The JSON Schema of `fraiseql.toml`, generated from [`TomlSchema`](super::TomlSchema), and
//! the walk that finds a key it does not take.
//!
//! The schema is the grammar's second statement, so it is generated from the type that reads
//! the file rather than written beside it. The walk reads a parsed file against it and names
//! the first key no table takes: where it was written, and every key that table does take.
//! It is a second reader of the file, so it refuses only where the schema says a table is
//! closed; a shape it cannot judge is left to serde, which refuses it in prose.
//!
//! A key the compiler refuses by name in every role the file plays (a retired spelling, a
//! key never applied, a spelling of the server's file) is in the schema as `"not": {}`, so an
//! editor flags it, with `"deprecated": true` and the compiler's own refusal as its
//! description.

use std::sync::OnceLock;

pub use fraiseql_core::utils::config_diagnostic::schema::UnknownKey;
use fraiseql_core::utils::config_diagnostic::schema::{self as config_schema, mark_refused};
use serde_json::Value;

/// Where the schema is committed, from the repository root.
pub const TOML_JSON_SCHEMA_PATH: &str = "docs/schemas/fraiseql.toml.schema.json";

/// The schema's URL for this release: the committed file at the release's tag, so a file
/// pointing at it is checked against the grammar of the compiler it was written for. It is
/// the schema's `$id`.
#[must_use]
pub fn toml_json_schema_url() -> String {
    format!(
        "https://raw.githubusercontent.com/fraiseql/fraiseql/v{}/{TOML_JSON_SCHEMA_PATH}",
        env!("CARGO_PKG_VERSION")
    )
}

/// The JSON Schema an editor checks `fraiseql.toml` against.
#[must_use]
pub fn toml_json_schema() -> Value {
    schema().clone()
}

fn schema() -> &'static Value {
    static SCHEMA: OnceLock<Value> = OnceLock::new();
    SCHEMA.get_or_init(|| {
        let mut schema =
            serde_json::to_value(schemars::schema_for!(super::TomlSchema)).unwrap_or_default();
        for refused in crate::config::refused_keys() {
            mark_refused(&mut schema, refused.section, refused.key, &refused.message);
        }
        schema["$id"] = Value::String(toml_json_schema_url());
        schema["title"] = Value::String("fraiseql.toml".to_owned());
        schema["description"] = Value::String(
            "The configuration the FraiseQL compiler reads: a project configuration beside a \
             schema document, or a schema authored in TOML."
                .to_owned(),
        );
        schema
    })
}

/// The first key in `file` that no table of `fraiseql.toml` takes, if any.
#[must_use]
pub fn unknown_key(file: &toml::Table) -> Option<UnknownKey> {
    config_schema::unknown_key(schema(), file)
}

#[cfg(test)]
mod tests;
