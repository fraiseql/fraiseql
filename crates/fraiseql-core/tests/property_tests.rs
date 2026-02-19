//! Property-based tests for FraiseQL core
//!
//! Uses proptest to verify invariants and properties that should hold
//! across all inputs and edge cases.

use fraiseql_core::{
    schema::{CompiledSchema, QueryDefinition, RoleDefinition, SecurityConfig, TypeDefinition},
    security::ErrorFormatter,
};
use proptest::prelude::*;
use serde_json::{Value, json};

// ============================================================================
// Property Tests for JSON Serialization
// ============================================================================

proptest! {
    /// Property: JSON serialization and deserialization should be invertible
    /// for null, bool, and string values (avoiding floating point precision issues)
    #[test]
    fn prop_json_roundtrip(value in arb_simple_json_value()) {
        // Serialize to string
        let serialized = serde_json::to_string(&value)
            .expect("JSON serialization failed");

        // Deserialize back
        let deserialized: Value = serde_json::from_str(&serialized)
            .expect("JSON deserialization failed");

        // Should be equal
        prop_assert_eq!(value, deserialized);
    }

    /// Property: Serializing twice should produce identical JSON strings
    #[test]
    fn prop_json_serialization_deterministic(value in arb_json_value()) {
        let json1 = serde_json::to_string(&value)
            .expect("first serialization failed");
        let json2 = serde_json::to_string(&value)
            .expect("second serialization failed");

        prop_assert_eq!(json1, json2, "JSON serialization should be deterministic");
    }
}

// ============================================================================
// Property Tests for String Escaping
// ============================================================================

proptest! {
    /// Property: Escaped SQL identifiers should be encapsulated in quotes
    #[test]
    fn prop_sql_identifier_escaping(identifier in "[a-zA-Z_][a-zA-Z0-9_]{0,50}") {
        let escaped = escape_sql_identifier(&identifier);

        // Should start and end with quotes
        prop_assert!(escaped.starts_with('"'), "Escaped identifier should start with quote");
        prop_assert!(escaped.ends_with('"'), "Escaped identifier should end with quote");

        // Original identifier characters should be present
        for c in identifier.chars() {
            prop_assert!(escaped.contains(c), "Identifier character lost in escaping");
        }
    }

    /// Property: SQL string values should be encapsulated in quotes after escaping
    #[test]
    fn prop_sql_string_escaping(value in "[ -~]{0,100}") {
        let escaped = escape_sql_string(&value);

        // Escaped string should be encapsulated in quotes
        prop_assert!(escaped.starts_with('\''), "Escaped string should start with quote");
        prop_assert!(escaped.ends_with('\''), "Escaped string should end with quote");
    }

    /// Property: Escaping should be consistent and reversible at protocol level
    #[test]
    fn prop_escaping_roundtrip(identifier in "[a-zA-Z_][a-zA-Z0-9_]{0,50}") {
        let escaped = escape_sql_identifier(&identifier);

        // Extract inner content (remove surrounding quotes)
        let inner = &escaped[1..escaped.len()-1];

        // Unescape double quotes back to single quotes
        let unescaped = inner.replace("\"\"", "\"");

        // Should match original identifier
        prop_assert_eq!(unescaped, identifier, "Escaping should be reversible");
    }
}

// ============================================================================
// Property Tests for Schema Operations
// ============================================================================

proptest! {
    /// Property: CompiledSchema JSON roundtrip preserves all data.
    /// Serializing a schema to JSON and deserializing it back should
    /// produce an equivalent schema (excluding runtime-only fields).
    #[test]
    fn prop_compiled_schema_json_roundtrip(
        type_names in prop::collection::vec("[A-Z][a-zA-Z]{0,20}", 0..5),
        query_names in prop::collection::vec("[a-z][a-zA-Z]{0,20}", 0..5),
    ) {
        let mut schema = CompiledSchema::new();
        for name in &type_names {
            schema.types.push(TypeDefinition::new(name.clone(), format!("v_{}", name.to_lowercase())));
        }
        for qname in &query_names {
            // Use a builtin return type so validation is not the concern here
            schema.queries.push(QueryDefinition::new(qname.clone(), "String"));
        }

        let json = schema.to_json().expect("serialization should succeed");
        let restored = CompiledSchema::from_json(&json).expect("deserialization should succeed");

        prop_assert_eq!(schema.types.len(), restored.types.len());
        prop_assert_eq!(schema.queries.len(), restored.queries.len());
        for (orig, rest) in schema.types.iter().zip(restored.types.iter()) {
            prop_assert_eq!(&orig.name, &rest.name);
            prop_assert_eq!(&orig.sql_source, &rest.sql_source);
        }
        for (orig, rest) in schema.queries.iter().zip(restored.queries.iter()) {
            prop_assert_eq!(&orig.name, &rest.name);
            prop_assert_eq!(&orig.return_type, &rest.return_type);
        }
    }

    /// Property: SecurityConfig JSON roundtrip preserves role definitions.
    #[test]
    fn prop_security_config_json_roundtrip(
        role_names in prop::collection::vec("[a-z_]{1,15}", 1..5),
        scopes in prop::collection::vec("[a-z]+:[a-zA-Z.*]+", 1..5),
    ) {
        let mut config = SecurityConfig::new();
        for name in &role_names {
            config.add_role(RoleDefinition::new(name.clone(), scopes.clone()));
        }

        let json = serde_json::to_string(&config).expect("serialization should succeed");
        let restored: SecurityConfig = serde_json::from_str(&json)
            .expect("deserialization should succeed");

        prop_assert_eq!(config.role_definitions.len(), restored.role_definitions.len());
        for (orig, rest) in config.role_definitions.iter().zip(restored.role_definitions.iter()) {
            prop_assert_eq!(&orig.name, &rest.name);
            prop_assert_eq!(&orig.scopes, &rest.scopes);
        }
    }

    /// Property: Schema validation detects duplicate type names deterministically.
    /// If we insert the same type name twice, validate() must always report an error.
    #[test]
    fn prop_schema_rejects_duplicate_type_names(
        name in "[A-Z][a-zA-Z]{1,20}",
    ) {
        let mut schema = CompiledSchema::new();
        schema.types.push(TypeDefinition::new(name.clone(), "v_table"));
        schema.types.push(TypeDefinition::new(name.clone(), "v_other"));

        let result = schema.validate();
        prop_assert!(result.is_err(), "Schema with duplicate type names should fail validation");
        let errors = result.unwrap_err();
        prop_assert!(
            errors.iter().any(|e| e.contains("Duplicate type name")),
            "Error should mention duplicate type name"
        );
    }

    /// Property: Production ErrorFormatter never leaks raw SQL in output.
    /// Any input containing SQL keywords should be sanitized when using
    /// production-level formatting.
    #[test]
    fn prop_error_formatter_hides_sql_in_production(
        prefix in "[a-zA-Z ]{0,30}",
        sql_keyword in prop_oneof![
            Just("SELECT "),
            Just("INSERT "),
            Just("UPDATE "),
            Just("DELETE "),
        ],
        suffix in "[a-zA-Z0-9_ ]{0,30}",
    ) {
        let formatter = ErrorFormatter::production();
        let raw_error = format!("{}{}{}", prefix, sql_keyword, suffix);
        let formatted = formatter.format_error(&raw_error);

        // Production formatter should not pass through raw SQL keywords
        prop_assert!(
            !formatted.contains(sql_keyword.trim()),
            "Production error should not contain SQL keyword '{}', got: {}",
            sql_keyword.trim(),
            formatted
        );
    }

    /// Property: Production ErrorFormatter never leaks database URLs in output.
    #[test]
    fn prop_error_formatter_hides_db_urls_in_production(
        user in "[a-zA-Z]{1,10}",
        host in "[a-zA-Z.]{1,15}",
        db_name in "[a-zA-Z_]{1,10}",
    ) {
        let formatter = ErrorFormatter::production();
        let raw_error = format!("Connection failed: postgresql://{}:secret@{}/{}", user, host, db_name);
        let formatted = formatter.format_error(&raw_error);

        // Production formatter should not expose the database URL
        prop_assert!(
            !formatted.contains("postgresql://"),
            "Production error should not contain database URL, got: {}",
            formatted
        );
    }
}

// ============================================================================
// Property Tests for Numeric Operations
// ============================================================================

proptest! {
    /// Property: JSON number serialization should preserve value
    #[test]
    fn prop_json_number_preservation(num in any::<i64>()) {
        let json = json!(num);
        let serialized = serde_json::to_string(&json).expect("serialization failed");
        let deserialized: i64 = serde_json::from_str(&serialized)
            .expect("deserialization failed");

        prop_assert_eq!(num, deserialized, "Number not preserved through JSON serialization");
    }

    /// Property: Float JSON serialization should be close to original value
    #[test]
    fn prop_json_float_roundtrip(
        num in 0.0f64..1_000_000.0,
        exponent in -308i32..308i32
    ) {
        let scaled = num * 10_f64.powi(exponent);
        prop_assume!(scaled.is_finite(), "Skip non-finite floats");

        let json = json!(scaled);
        let serialized = serde_json::to_string(&json).expect("serialization failed");
        let deserialized: f64 = serde_json::from_str(&serialized)
            .expect("deserialization failed");

        // Floats may not be exactly equal, but should be very close
        let difference = (scaled - deserialized).abs();
        let tolerance = scaled.abs() * 1e-15 + 1e-15;
        prop_assert!(difference < tolerance, "Float not preserved in JSON serialization");
    }
}

// ============================================================================
// Helper Functions and Strategies
// ============================================================================

/// Strategy for generating simple JSON values (no floats to avoid precision issues)
fn arb_simple_json_value() -> impl Strategy<Value = Value> {
    let leaf = prop_oneof![
        Just(Value::Null),
        any::<bool>().prop_map(Value::Bool),
        any::<i64>().prop_map(|n| json!(n)),
        any::<String>().prop_map(Value::String),
    ];

    leaf.prop_recursive(
        4,   // max depth
        256, // max nodes
        10,  // items per collection
        |inner| {
            prop_oneof![
                // JSON arrays
                prop::collection::vec(inner.clone(), 0..10).prop_map(Value::Array),
                // JSON objects
                prop::collection::hash_map("[a-zA-Z][a-zA-Z0-9_]*", inner, 0..10).prop_map(|map| {
                    let mut obj = serde_json::Map::new();
                    for (k, v) in map {
                        obj.insert(k, v);
                    }
                    Value::Object(obj)
                }),
            ]
        },
    )
}

/// Strategy for generating arbitrary JSON values (including floats)
fn arb_json_value() -> impl Strategy<Value = Value> {
    let leaf = prop_oneof![
        Just(Value::Null),
        any::<bool>().prop_map(Value::Bool),
        any::<i64>().prop_map(|n| json!(n)),
        any::<f64>()
            .prop_filter("finite floats only", |f| f.is_finite())
            .prop_map(|f| json!(f)),
        any::<String>().prop_map(Value::String),
    ];

    leaf.prop_recursive(
        4,   // max depth
        256, // max nodes
        10,  // items per collection
        |inner| {
            prop_oneof![
                // JSON arrays
                prop::collection::vec(inner.clone(), 0..10).prop_map(Value::Array),
                // JSON objects
                prop::collection::hash_map("[a-zA-Z][a-zA-Z0-9_]*", inner, 0..10).prop_map(|map| {
                    let mut obj = serde_json::Map::new();
                    for (k, v) in map {
                        obj.insert(k, v);
                    }
                    Value::Object(obj)
                }),
            ]
        },
    )
}

/// Escape a SQL identifier (table name, column name, etc.)
fn escape_sql_identifier(ident: &str) -> String {
    format!("\"{}\"", ident.replace('"', "\"\""))
}

/// Escape a SQL string value
fn escape_sql_string(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_escape_sql_identifier() {
        assert_eq!(escape_sql_identifier("users"), "\"users\"");
        assert_eq!(escape_sql_identifier("my\"table"), "\"my\"\"table\"");
    }

    #[test]
    fn test_escape_sql_string() {
        assert_eq!(escape_sql_string("hello"), "'hello'");
        assert_eq!(escape_sql_string("O'Brien"), "'O''Brien'");
    }
}
