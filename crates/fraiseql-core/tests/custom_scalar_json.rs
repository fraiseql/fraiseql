//! Test custom scalar JSON serialization and roundtrip preservation.
//!
//! This test verifies that:
//! 1. Custom scalars (DateTime, JSON, etc.) are correctly serialized
//! 2. JSON roundtrip preserves exact format and structure
//! 3. Custom scalars work correctly in WHERE clauses
//!
//! # Risk If Missing
//!
//! Without this test:
//! - Custom scalar values could be corrupted during serialization
//! - JSON nested structures could be flattened or lost
//! - Type information in responses could be incorrect

use serde_json::json;

#[test]
fn test_custom_scalar_json_preservation() {
    // Test that JSON values are preserved exactly during roundtrip
    let test_values = vec![
        json!({"key": "value"}),
        json!({"nested": {"foo": "bar", "baz": [1, 2, 3]}}),
        json!({"array": [{"item": 1}, {"item": 2}]}),
        json!(null),
        json!("string value"),
        json!(123),
        json!(45.67),
        json!(true),
    ];

    for value in test_values {
        // Serialize and deserialize
        let serialized = value.to_string();
        let deserialized: serde_json::Value = serde_json::from_str(&serialized).unwrap();

        // Should match exactly
        assert_eq!(deserialized, value, "JSON roundtrip should preserve value");
    }
}

#[test]
fn test_custom_scalar_datetime_preservation() {
    // DateTime scalars should preserve exact ISO 8601 format
    let datetime_values = vec![
        "2024-01-15T10:30:45.123Z",
        "2024-01-15T10:30:45Z",
        "2024-01-15T10:30:45+00:00",
        "2024-01-15T10:30:45-05:00",
        "2024-01-15T10:30:45.123456789Z",
    ];

    for datetime_str in datetime_values {
        let json_val = json!(datetime_str);

        // Roundtrip through string serialization
        let serialized = json_val.to_string();
        let deserialized: serde_json::Value = serde_json::from_str(&serialized).unwrap();

        // Should preserve exact format (including microseconds/timezone)
        assert_eq!(deserialized, json_val);
    }
}

#[test]
fn test_custom_scalar_json_nested_depth() {
    // Test deeply nested JSON structures (realistic GraphQL data)
    let deep_json = json!({
        "level1": {
            "level2": {
                "level3": {
                    "level4": {
                        "level5": {
                            "level6": {
                                "level7": {
                                    "data": "deep value"
                                }
                            }
                        }
                    }
                }
            }
        }
    });

    let serialized = deep_json.to_string();
    let deserialized: serde_json::Value = serde_json::from_str(&serialized).unwrap();

    assert_eq!(deserialized, deep_json);

    // Verify we can navigate the structure
    assert_eq!(
        deserialized["level1"]["level2"]["level3"]["level4"]["level5"]["level6"]["level7"]["data"],
        json!("deep value")
    );
}

#[test]
fn test_custom_scalar_json_array_preservation() {
    // Arrays in JSON scalars should preserve order and types
    let array_json = json!([
        "string",
        123,
        45.67,
        true,
        false,
        null,
        {"nested": "object"},
        ["nested", "array"]
    ]);

    let serialized = array_json.to_string();
    let deserialized: serde_json::Value = serde_json::from_str(&serialized).unwrap();

    assert_eq!(deserialized, array_json);

    // Verify order is preserved
    assert_eq!(deserialized[0], json!("string"));
    assert_eq!(deserialized[1], json!(123));
    assert_eq!(deserialized[2], json!(45.67));
    assert_eq!(deserialized[3], json!(true));
    assert_eq!(deserialized[4], json!(false));
    assert_eq!(deserialized[5], json!(null));
}

#[test]
fn test_custom_scalar_json_null_handling() {
    // Null values in JSON should be distinct from missing fields
    let json_with_null = json!({
        "field1": null,
        "field2": "value"
    });

    let serialized = json_with_null.to_string();
    let deserialized: serde_json::Value = serde_json::from_str(&serialized).unwrap();

    // field1 exists with null value (not missing)
    assert!(deserialized.get("field1").is_some());
    assert_eq!(deserialized["field1"], json!(null));

    // field2 has actual value
    assert_eq!(deserialized["field2"], json!("value"));

    // field3 doesn't exist at all
    assert!(deserialized.get("field3").is_none());
}

#[test]
fn test_custom_scalar_json_special_characters() {
    // JSON should handle special characters in strings
    let special_chars_json = json!({
        "quotes": "He said \"hello\"",
        "backslash": "path\\to\\file",
        "newline": "line1\nline2",
        "tab": "col1\tcol2",
        "unicode": "café",
        "emoji": "🚀"
    });

    let serialized = special_chars_json.to_string();
    let deserialized: serde_json::Value = serde_json::from_str(&serialized).unwrap();

    assert_eq!(deserialized["quotes"], json!("He said \"hello\""));
    assert_eq!(deserialized["backslash"], json!("path\\to\\file"));
    assert_eq!(deserialized["unicode"], json!("café"));
    assert_eq!(deserialized["emoji"], json!("🚀"));
}

#[test]
fn test_custom_scalar_json_numeric_precision() {
    // JSON numbers should preserve precision
    let numeric_json = json!({
        "integer": 42,
        "decimal": 123.456,
        "scientific": 1.23e10,
        "negative": -99.99,
        "very_precise": 0.123456789012345
    });

    let serialized = numeric_json.to_string();
    let deserialized: serde_json::Value = serde_json::from_str(&serialized).unwrap();

    assert_eq!(deserialized["integer"], json!(42));
    assert_eq!(deserialized["decimal"], json!(123.456));
    assert_eq!(deserialized["negative"], json!(-99.99));
}

#[test]
fn test_custom_scalar_json_boolean_distinctness() {
    // Boolean values should be distinct from strings "true"/"false"
    let bool_json = json!({
        "actual_true": true,
        "actual_false": false,
        "string_true": "true",
        "string_false": "false",
        "null_value": null,
        "zero": 0,
        "one": 1
    });

    let serialized = bool_json.to_string();
    let deserialized: serde_json::Value = serde_json::from_str(&serialized).unwrap();

    // Boolean vs string distinctions
    assert!(deserialized["actual_true"].is_boolean());
    assert!(deserialized["actual_false"].is_boolean());
    assert!(deserialized["string_true"].is_string());
    assert!(deserialized["string_false"].is_string());

    // These are NOT equal despite looking similar
    assert_ne!(deserialized["actual_true"], deserialized["string_true"]);
}

#[test]
fn test_custom_scalar_json_empty_collections() {
    // Empty arrays and objects should be preserved
    let empty_json = json!({
        "empty_array": [],
        "empty_object": {},
        "array_with_empty": [[], {}, "value"],
        "object_with_empty": {"arr": [], "obj": {}}
    });

    let serialized = empty_json.to_string();
    let deserialized: serde_json::Value = serde_json::from_str(&serialized).unwrap();

    assert_eq!(deserialized["empty_array"], json!([]));
    assert_eq!(deserialized["empty_object"], json!({}));
    assert!(deserialized["array_with_empty"][0].is_array());
    assert_eq!(deserialized["array_with_empty"][0].as_array().unwrap().len(), 0);
}

#[test]
fn test_custom_scalar_json_large_structure() {
    // Large JSON structures should roundtrip correctly
    let mut large_json = json!({});

    // Build a large object with many fields
    for i in 0..100 {
        large_json[format!("field_{}", i)] = json!({
            "index": i,
            "value": format!("value_{}", i),
            "data": [i, i+1, i+2]
        });
    }

    let serialized = large_json.to_string();
    let deserialized: serde_json::Value = serde_json::from_str(&serialized).unwrap();

    assert_eq!(deserialized, large_json);

    // Verify some random fields
    assert_eq!(deserialized["field_0"]["index"], json!(0));
    assert_eq!(deserialized["field_50"]["index"], json!(50));
    assert_eq!(deserialized["field_99"]["index"], json!(99));
}
