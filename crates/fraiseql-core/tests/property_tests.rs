//! Property-based tests for FraiseQL core
//!
//! Uses proptest to verify invariants and properties that should hold
//! across all inputs and edge cases.

use proptest::prelude::*;
use serde_json::{Value, json};
use std::collections::HashMap;

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
// Property Tests for Collection Operations
// ============================================================================

proptest! {
    /// Property: HashMap insert should make the value retrievable
    #[test]
    fn prop_hashmap_insert_retrieve(
        key in "[a-zA-Z0-9_]+",
        value in any::<i32>()
    ) {
        let mut map: HashMap<String, i32> = HashMap::new();
        map.insert(key.clone(), value);

        // Should be able to retrieve the inserted value
        prop_assert_eq!(map.get(&key), Some(&value));
    }

    /// Property: HashMap length should match number of unique keys
    #[test]
    fn prop_hashmap_length(
        kvs in prop::collection::hash_map("[a-zA-Z0-9_]+", any::<i32>(), 0..100)
    ) {
        let map: HashMap<String, i32> = kvs.clone();
        prop_assert_eq!(map.len(), kvs.len(), "HashMap length should match input");
    }

    /// Property: HashMap iteration should visit all entries
    #[test]
    fn prop_hashmap_iteration_coverage(
        kvs in prop::collection::hash_map("[a-zA-Z0-9_]+", any::<i32>(), 0..100)
    ) {
        let map: HashMap<String, i32> = kvs.clone();
        let mut visited = std::collections::HashSet::new();

        for (k, v) in map.iter() {
            visited.insert((k.clone(), *v));
        }

        prop_assert_eq!(visited.len(), map.len(), "Iteration should visit all entries");

        // All visited entries should be in the original map
        for (k, v) in visited {
            prop_assert_eq!(map.get(&k), Some(&v), "Visited entry not in map");
        }
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
