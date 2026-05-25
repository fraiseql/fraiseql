//! Helper functions for REST parameter extraction.

use fraiseql_core::schema::TypeDefinition;
use fraiseql_error::FraiseQLError;

/// Compute the nesting depth of a JSON value.
pub fn json_depth(value: &serde_json::Value) -> usize {
    match value {
        serde_json::Value::Object(map) => 1 + map.values().map(json_depth).max().unwrap_or(0),
        serde_json::Value::Array(arr) => 1 + arr.iter().map(json_depth).max().unwrap_or(0),
        _ => 1,
    }
}

/// Count the number of field-level entries in a WHERE clause value.
#[must_use]
pub fn count_where_fields(value: &serde_json::Value) -> usize {
    match value.as_object() {
        Some(map) => map.len(),
        None => 1,
    }
}

/// Get sorted field output names from a `TypeDefinition`.
#[must_use]
pub fn field_names(td: &TypeDefinition) -> Vec<&str> {
    let mut names: Vec<&str> = td.fields.iter().map(|f| f.output_name()).collect();
    names.sort_unstable();
    names
}

/// Convenience constructor for `FraiseQLError::Validation`.
#[must_use]
pub const fn validation_error(message: String) -> FraiseQLError {
    FraiseQLError::Validation {
        message,
        path: None,
    }
}
