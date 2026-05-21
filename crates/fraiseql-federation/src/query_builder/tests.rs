#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable
#![allow(clippy::iter_on_single_items)] // Reason: test data uses single-element iter for structural clarity

use std::collections::HashMap;

use serde_json::json;

use super::*;

fn make_test_metadata() -> FederationMetadata {
    use crate::types::{FederatedType, KeyDirective};

    let types = vec![FederatedType {
        name:                "User".to_string(),
        keys:                vec![KeyDirective {
            fields:     vec!["id".to_string()],
            resolvable: true,
        }],
        is_extends:          false,
        external_fields:     vec![],
        shareable_fields:    vec![],
        inaccessible_fields: vec![],
        field_directives:    std::collections::HashMap::new(),
        type_shareable:      false,
    }];

    FederationMetadata {
        enabled: true,
        version: "v2".to_string(),
        types,
        remote_subscription_fields: HashMap::new(),
    }
}

#[test]
fn test_construct_simple_where_in() {
    let metadata = make_test_metadata();
    let reps = vec![
        EntityRepresentation {
            typename:   "User".to_string(),
            key_fields: [(String::from("id"), json!("123"))].iter().cloned().collect(),
            all_fields: HashMap::default(),
        },
        EntityRepresentation {
            typename:   "User".to_string(),
            key_fields: [(String::from("id"), json!("456"))].iter().cloned().collect(),
            all_fields: HashMap::default(),
        },
    ];

    let clause = construct_where_in_clause("User", &reps, &metadata).unwrap();
    assert!(clause.contains("id IN"));
    assert!(clause.contains("'123'"));
    assert!(clause.contains("'456'"));
}

#[test]
fn test_sql_injection_prevention() {
    let metadata = make_test_metadata();
    let reps = vec![EntityRepresentation {
        typename:   "User".to_string(),
        key_fields: [(String::from("id"), json!("'; DROP TABLE users; --"))]
            .iter()
            .cloned()
            .collect(),
        all_fields: HashMap::default(),
    }];

    let clause = construct_where_in_clause("User", &reps, &metadata).unwrap();
    // Dangerous SQL should be escaped
    assert!(clause.contains("'';")); // Single quote should be doubled
}

#[test]
fn test_escape_sql_string() {
    let result = escape_sql_string("O'Brien");
    assert_eq!(result, "O''Brien");

    let result = escape_sql_string("test''; DROP--");
    assert_eq!(result, "test''''; DROP--");
}

#[test]
fn test_empty_representations() {
    let metadata = make_test_metadata();
    let reps = vec![];

    let clause = construct_where_in_clause("User", &reps, &metadata).unwrap();
    assert_eq!(clause, "1 = 0"); // No rows to resolve
}

#[test]
fn test_missing_type_error() {
    let metadata = make_test_metadata();
    let reps = vec![];

    let result = construct_where_in_clause("NotFound", &reps, &metadata);
    assert!(
        matches!(result, Err(FraiseQLError::Validation { .. })),
        "expected Validation error for unknown type, got: {result:?}"
    );
}
