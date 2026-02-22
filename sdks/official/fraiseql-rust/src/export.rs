//! Schema export — serialise the global [`SchemaRegistry`] to `schema.json`.
//!
//! These functions are the Rust equivalent of `fraiseql.export_schema()` in
//! Python or `exportSchema()` in TypeScript.

use std::path::Path;
use crate::registry::SchemaRegistry;

/// Errors that can occur during schema export.
#[derive(Debug)]
pub enum ExportError {
    Io(std::io::Error),
    Serialization(serde_json::Error),
}

impl std::fmt::Display for ExportError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(e) => write!(f, "IO error: {e}"),
            Self::Serialization(e) => write!(f, "Serialization error: {e}"),
        }
    }
}

impl From<std::io::Error> for ExportError {
    fn from(e: std::io::Error) -> Self { Self::Io(e) }
}

impl From<serde_json::Error> for ExportError {
    fn from(e: serde_json::Error) -> Self { Self::Serialization(e) }
}

/// Export the full schema (types + operations) to `schema.json`.
///
/// This file is the input to `fraiseql-cli compile`.
///
/// # Example
///
/// ```rust,ignore
/// fn main() {
///     fraiseql_rust::export::export_schema("schema.json").unwrap();
/// }
/// ```
pub fn export_schema(path: impl AsRef<Path>) -> Result<(), ExportError> {
    let json = schema_to_json()?;
    std::fs::write(path, json)?;
    Ok(())
}

/// Export type definitions only to `types.json`.
///
/// Useful when sharing type information with other tooling without
/// exposing query/mutation details.
pub fn export_types(path: impl AsRef<Path>) -> Result<(), ExportError> {
    let json = types_to_json()?;
    std::fs::write(path, json)?;
    Ok(())
}

/// Serialise the full schema to a JSON string without writing to disk.
pub fn schema_to_json() -> Result<String, ExportError> {
    SchemaRegistry::with(|registry| {
        let queries: Vec<_> = registry.operations.iter()
            .filter(|o| matches!(o.kind, crate::registry::OperationKind::Query))
            .collect();
        let mutations: Vec<_> = registry.operations.iter()
            .filter(|o| matches!(o.kind, crate::registry::OperationKind::Mutation))
            .collect();
        let subscriptions: Vec<_> = registry.operations.iter()
            .filter(|o| matches!(o.kind, crate::registry::OperationKind::Subscription))
            .collect();
        let doc = serde_json::json!({
            "version": "2.0",
            "types": registry.types,
            "queries": queries,
            "mutations": mutations,
            "subscriptions": subscriptions,
        });
        serde_json::to_string_pretty(&doc).map_err(ExportError::Serialization)
    })
}

/// Serialise type definitions only to a JSON string.
pub fn types_to_json() -> Result<String, ExportError> {
    SchemaRegistry::with(|registry| {
        let doc = serde_json::json!({
            "version": "2.0",
            "types": registry.types,
        });
        serde_json::to_string_pretty(&doc).map_err(ExportError::Serialization)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_schema_to_json_produces_valid_json() {
        let json = schema_to_json().expect("export should succeed");
        let parsed: serde_json::Value =
            serde_json::from_str(&json).expect("output should be valid JSON");
        assert_eq!(parsed["version"], "2.0");
        assert!(parsed["types"].is_array());
        assert!(parsed["queries"].is_array());
        assert!(parsed["mutations"].is_array());
    }

    #[test]
    fn test_types_to_json_omits_operations() {
        let json = types_to_json().expect("export should succeed");
        let parsed: serde_json::Value =
            serde_json::from_str(&json).expect("output should be valid JSON");
        assert!(parsed.get("queries").is_none());
        assert!(parsed.get("mutations").is_none());
        assert!(parsed["types"].is_array());
    }
}
