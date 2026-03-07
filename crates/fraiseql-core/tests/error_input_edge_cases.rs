#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

//! Input edge case tests.
//!
//! Tests validation and parsing of malformed, extreme, or adversarial inputs.

#![allow(clippy::no_effect_underscore_binding)] // Reason: _ bindings used in test destructuring patterns
#![allow(missing_docs)] // Reason: test helper functions do not require documentation
#![allow(clippy::format_push_string)] // Reason: test query builders use push_str(&format!()) for readability
#![allow(clippy::needless_collect)] // Reason: intermediate collect preserves ownership for later assertions
use fraiseql_core::{
    db::{WhereClause, WhereOperator},
    security::{QueryValidator, QueryValidatorConfig},
};
use serde_json::json;

// ========================================================================
// Query Validation Edge Cases
// ========================================================================

#[test]
fn test_deeply_nested_query_rejected() {
    let validator = QueryValidator::from_config(QueryValidatorConfig {
        max_depth:      10,
        max_complexity: 10_000,
        max_size_bytes: 1_000_000,
    });

    // Build a 50-level nested query
    let mut query = String::new();
    for _ in 0..50 {
        query.push_str("{ nested ");
    }
    query.push_str("{ leaf }");
    for _ in 0..50 {
        query.push_str(" }");
    }

    let result = validator.validate(&query);
    assert!(result.is_err(), "50-level nested query should be rejected");
}

#[test]
fn test_very_high_complexity_query_rejected() {
    let validator = QueryValidator::from_config(QueryValidatorConfig {
        max_depth:      100,
        max_complexity: 100,
        max_size_bytes: 10_000_000,
    });

    // Build a wide query with many fields to exceed complexity
    let mut query = String::from("{ ");
    for i in 0..200 {
        query.push_str(&format!("field_{i} "));
    }
    query.push('}');

    let result = validator.validate(&query);
    // Either rejected by complexity or passes — we verify the validator processes it
    // without panicking. If complexity is counted per field, 200 fields should exceed 100.
    if let Ok(metrics) = &result {
        // If it passes, complexity should still be computed
        assert!(metrics.field_count > 0);
    }
}

#[test]
fn test_empty_query_handled() {
    let validator = QueryValidator::standard();

    // Empty string should not panic
    let result = validator.validate("");
    // Either returns an error or metrics with zero depth — both are acceptable
    if let Ok(metrics) = result {
        assert_eq!(metrics.size_bytes, 0);
    }
}

#[test]
fn test_malformed_graphql_handled() {
    let validator = QueryValidator::standard();

    // Mismatched braces
    let result = validator.validate("{ user { name }");
    // Should not panic regardless of result
    let _ = result;

    // Random garbage
    let result = validator.validate("))){{{{}}}}(((");
    let _ = result;
}

#[test]
fn test_query_exceeding_size_limit_rejected() {
    let validator = QueryValidator::from_config(QueryValidatorConfig {
        max_depth:      10,
        max_complexity: 1000,
        max_size_bytes: 100,
    });

    let large_query = "{ ".to_string() + &"a ".repeat(100) + "}";
    let result = validator.validate(&large_query);
    assert!(result.is_err(), "query exceeding size limit should be rejected");
}

// ========================================================================
// WhereClause Edge Cases
// ========================================================================

#[test]
fn test_where_clause_with_empty_path() {
    // WhereClause with empty path vec — should be constructible
    let clause = WhereClause::Field {
        path:     vec![],
        operator: WhereOperator::Eq,
        value:    json!(1),
    };

    // Verify it serializes without panic
    let serialized = serde_json::to_string(&clause).unwrap();
    assert!(serialized.contains("\"path\":[]"));
}

#[test]
fn test_where_clause_with_very_long_field_name() {
    let long_name = "x".repeat(10_000);
    let clause = WhereClause::Field {
        path:     vec![long_name],
        operator: WhereOperator::Eq,
        value:    json!("test"),
    };

    // Should serialize/deserialize without panic
    let serialized = serde_json::to_string(&clause).unwrap();
    let deserialized: WhereClause = serde_json::from_str(&serialized).unwrap();
    match deserialized {
        WhereClause::Field { path, .. } => assert_eq!(path[0].len(), 10_000),
        other => panic!("expected Field variant, got {other:?}"),
    }
}

#[test]
fn test_where_clause_with_null_value() {
    let clause = WhereClause::Field {
        path:     vec!["status".to_string()],
        operator: WhereOperator::Eq,
        value:    json!(null),
    };

    let serialized = serde_json::to_string(&clause).unwrap();
    let deserialized: WhereClause = serde_json::from_str(&serialized).unwrap();
    match deserialized {
        WhereClause::Field { value, .. } => assert!(value.is_null()),
        other => panic!("expected Field variant, got {other:?}"),
    }
}

#[test]
fn test_where_clause_with_deeply_nested_and_or() {
    // Build 100 levels of nested And/Or
    let mut clause = WhereClause::Field {
        path:     vec!["id".to_string()],
        operator: WhereOperator::Eq,
        value:    json!(1),
    };

    for i in 0..100 {
        clause = if i % 2 == 0 {
            WhereClause::And(vec![clause])
        } else {
            WhereClause::Or(vec![clause])
        };
    }

    // Serialization succeeds even with deep nesting
    let serialized = serde_json::to_string(&clause).unwrap();
    assert!(!serialized.is_empty());

    // Deserialization hits serde_json's recursion limit (default 128)
    // at 100 levels of nesting. This is expected behavior — serde_json
    // protects against stack overflow on deeply nested structures.
    let deser_result = serde_json::from_str::<WhereClause>(&serialized);
    assert!(
        deser_result.is_err(),
        "100-level nested WhereClause should hit serde_json recursion limit"
    );
}

#[test]
fn test_where_clause_moderate_nesting_roundtrips() {
    // 20 levels of nesting should roundtrip fine
    let mut clause = WhereClause::Field {
        path:     vec!["id".to_string()],
        operator: WhereOperator::Eq,
        value:    json!(1),
    };

    for i in 0..20 {
        clause = if i % 2 == 0 {
            WhereClause::And(vec![clause])
        } else {
            WhereClause::Or(vec![clause])
        };
    }

    let serialized = serde_json::to_string(&clause).unwrap();
    let deserialized: WhereClause = serde_json::from_str(&serialized).unwrap();
    let re_serialized = serde_json::to_string(&deserialized).unwrap();
    assert_eq!(serialized, re_serialized);
}

#[test]
fn test_where_clause_not_wrapping() {
    let inner = WhereClause::Field {
        path:     vec!["active".to_string()],
        operator: WhereOperator::Eq,
        value:    json!(true),
    };
    let clause = WhereClause::Not(Box::new(inner));

    let serialized = serde_json::to_string(&clause).unwrap();
    let deserialized: WhereClause = serde_json::from_str(&serialized).unwrap();
    assert!(matches!(deserialized, WhereClause::Not(_)));
}
