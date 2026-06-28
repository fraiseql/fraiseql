#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable
#![allow(clippy::iter_on_single_items)] // Reason: test data uses single-element iter for structural clarity

use std::collections::HashMap;

use fraiseql_db::DatabaseType;
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

fn rep(id: &str) -> EntityRepresentation {
    EntityRepresentation {
        typename:   "User".to_string(),
        key_fields: [(String::from("id"), json!(id))].iter().cloned().collect(),
        all_fields: HashMap::default(),
    }
}

#[test]
fn test_construct_simple_where_in_binds_values() {
    let metadata = make_test_metadata();
    let reps = vec![rep("123"), rep("456")];

    let (clause, params) =
        construct_where_in_clause("User", &reps, &metadata, DatabaseType::PostgreSQL).unwrap();

    // Values are bound, not interpolated: the clause carries placeholders only.
    // The key column is cast to text on PostgreSQL so a text-bound key matches a
    // non-text (e.g. uuid) key column (#504).
    assert_eq!(clause, "id::text IN ($1, $2)");
    assert!(!clause.contains("123"));
    assert!(!clause.contains("456"));
    assert_eq!(params, vec![json!("123"), json!("456")]);
}

#[test]
fn test_dialect_placeholders() {
    let metadata = make_test_metadata();
    let reps = vec![rep("a")];
    let (pg, _) =
        construct_where_in_clause("User", &reps, &metadata, DatabaseType::PostgreSQL).unwrap();
    // PostgreSQL casts the key column to text (#504); other dialects coerce.
    assert_eq!(pg, "id::text IN ($1)");
    let (my, _) = construct_where_in_clause("User", &reps, &metadata, DatabaseType::MySQL).unwrap();
    assert_eq!(my, "id IN (?)");
    let (ms, _) =
        construct_where_in_clause("User", &reps, &metadata, DatabaseType::SQLServer).unwrap();
    assert_eq!(ms, "id IN (@P1)");
}

#[test]
fn test_sql_injection_value_is_bound_not_interpolated() {
    // The injection payload — including the MySQL backslash-breakout vector — must
    // be carried as a bound parameter, never spliced into the SQL text (H3).
    let metadata = make_test_metadata();
    let payload = r"\'; DROP TABLE users; --";
    let reps = vec![rep(payload)];

    let (clause, params) =
        construct_where_in_clause("User", &reps, &metadata, DatabaseType::MySQL).unwrap();

    assert_eq!(clause, "id IN (?)");
    assert!(!clause.contains("DROP"), "payload must not appear in SQL text");
    assert!(!clause.contains('\''), "no inline quotes in the parameterized clause");
    assert_eq!(params, vec![json!(payload)]);
}

#[test]
fn test_empty_representations() {
    let metadata = make_test_metadata();
    let reps = vec![];

    let (clause, params) =
        construct_where_in_clause("User", &reps, &metadata, DatabaseType::PostgreSQL).unwrap();
    assert_eq!(clause, "1 = 0"); // No rows to resolve
    assert!(params.is_empty());
}

#[test]
fn test_missing_type_error() {
    let metadata = make_test_metadata();
    let reps = vec![];

    let result = construct_where_in_clause("NotFound", &reps, &metadata, DatabaseType::PostgreSQL);
    assert!(
        matches!(result, Err(FraiseQLError::Validation { .. })),
        "expected Validation error for unknown type, got: {result:?}"
    );
}

/// M-batch-where-dup: compound `@key` resolution goes through the safe,
/// parameterized canonical builder (`(k1, k2) IN ((…), …)`) — coverage ported
/// from the deleted `construct_batch_where_clause` duplicate, which interpolated
/// values as string literals and produced a WHERE-less clause when empty.
#[test]
fn test_construct_composite_where_in_binds_values() {
    use crate::types::{FederatedType, KeyDirective};

    let metadata = FederationMetadata {
        enabled: true,
        version: "v2".to_string(),
        types: vec![FederatedType {
            name:                "OrderItem".to_string(),
            keys:                vec![KeyDirective {
                fields:     vec!["order_id".to_string(), "product_id".to_string()],
                resolvable: true,
            }],
            is_extends:          false,
            external_fields:     vec![],
            shareable_fields:    vec![],
            inaccessible_fields: vec![],
            field_directives:    HashMap::new(),
            type_shareable:      false,
        }],
        remote_subscription_fields: HashMap::new(),
    };

    let rep = EntityRepresentation {
        typename:   "OrderItem".to_string(),
        key_fields: [
            (String::from("order_id"), json!("O1")),
            (String::from("product_id"), json!("P1")),
        ]
        .iter()
        .cloned()
        .collect(),
        all_fields: HashMap::default(),
    };

    let (clause, params) =
        construct_where_in_clause("OrderItem", &[rep], &metadata, DatabaseType::PostgreSQL)
            .unwrap();

    // Each composite key column is cast to text on PostgreSQL (#504).
    assert_eq!(clause, "(order_id::text, product_id::text) IN (($1, $2))");
    // Values are bound as parameters, never interpolated into the SQL text.
    assert!(!clause.contains("O1") && !clause.contains("P1"));
    assert_eq!(params, vec![json!("O1"), json!("P1")]);
}
