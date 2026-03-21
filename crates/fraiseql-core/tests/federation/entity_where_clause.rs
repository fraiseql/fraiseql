//! WHERE clause construction and SQL injection prevention tests.
//!
//! Split from `federation_database_integration.rs`.

#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable
#![allow(clippy::cast_possible_truncation)] // Reason: test uses usize→u32 for small test counts
use std::collections::HashMap;

use fraiseql_core::federation::{
    query_builder::construct_where_in_clause, types::EntityRepresentation,
};
use fraiseql_db::DatabaseType;
use serde_json::json;

use super::common;

// ============================================================================
// WHERE Clause Construction (Parameterized)
// ============================================================================

#[test]
fn test_where_clause_single_key_field() {
    let metadata = common::metadata_single_key("User", "id");

    let mut rep1_keys = HashMap::new();
    rep1_keys.insert("id".to_string(), json!("123"));
    let mut rep1_all = HashMap::new();
    rep1_all.insert("id".to_string(), json!("123"));
    let rep1 = EntityRepresentation {
        typename:   "User".to_string(),
        key_fields: rep1_keys,
        all_fields: rep1_all,
    };

    let mut rep2_keys = HashMap::new();
    rep2_keys.insert("id".to_string(), json!("456"));
    let mut rep2_all = HashMap::new();
    rep2_all.insert("id".to_string(), json!("456"));
    let rep2 = EntityRepresentation {
        typename:   "User".to_string(),
        key_fields: rep2_keys,
        all_fields: rep2_all,
    };

    let result =
        construct_where_in_clause("User", &[rep1, rep2], &metadata, DatabaseType::PostgreSQL)
            .unwrap();
    assert_eq!(result.sql, "id IN ($1, $2)");
    assert_eq!(result.params, vec![json!("123"), json!("456")]);
}

#[test]
fn test_where_clause_composite_keys() {
    let metadata = common::metadata_composite_key("Order", &["user_id", "order_id"]);

    let mut rep1_keys = HashMap::new();
    rep1_keys.insert("user_id".to_string(), json!("user1"));
    rep1_keys.insert("order_id".to_string(), json!("order1"));
    let mut rep1_all = HashMap::new();
    rep1_all.insert("user_id".to_string(), json!("user1"));
    rep1_all.insert("order_id".to_string(), json!("order1"));
    let rep1 = EntityRepresentation {
        typename:   "Order".to_string(),
        key_fields: rep1_keys,
        all_fields: rep1_all,
    };

    let result =
        construct_where_in_clause("Order", &[rep1], &metadata, DatabaseType::PostgreSQL).unwrap();
    assert_eq!(result.sql, "(user_id, order_id) IN (($1, $2))");
    assert_eq!(result.params, vec![json!("user1"), json!("order1")]);
}

#[test]
fn test_where_clause_string_escaping_not_needed() {
    // With parameterized queries, values with special characters go in params, not SQL
    let metadata = common::metadata_single_key("User", "name");

    let mut rep_keys = HashMap::new();
    rep_keys.insert("name".to_string(), json!("O'Brien"));
    let mut rep_all = HashMap::new();
    rep_all.insert("name".to_string(), json!("O'Brien"));
    let rep = EntityRepresentation {
        typename:   "User".to_string(),
        key_fields: rep_keys,
        all_fields: rep_all,
    };

    let result =
        construct_where_in_clause("User", &[rep], &metadata, DatabaseType::PostgreSQL).unwrap();
    assert_eq!(result.sql, "name IN ($1)");
    assert_eq!(result.params, vec![json!("O'Brien")]);
    // The dangerous character is in params, not in SQL text
    assert!(!result.sql.contains('\''));
}

#[test]
fn test_where_clause_sql_injection_prevention() {
    let metadata = common::metadata_single_key("User", "id");

    let mut rep_keys = HashMap::new();
    rep_keys.insert("id".to_string(), json!("'; DROP TABLE users; --"));
    let mut rep_all = HashMap::new();
    rep_all.insert("id".to_string(), json!("'; DROP TABLE users; --"));
    let rep = EntityRepresentation {
        typename:   "User".to_string(),
        key_fields: rep_keys,
        all_fields: rep_all,
    };

    let result =
        construct_where_in_clause("User", &[rep], &metadata, DatabaseType::PostgreSQL).unwrap();
    // SQL text contains only placeholders, no user values
    assert_eq!(result.sql, "id IN ($1)");
    assert!(!result.sql.contains("DROP"));
    // The malicious value is safely in params
    assert_eq!(result.params, vec![json!("'; DROP TABLE users; --")]);
}

#[test]
fn test_where_clause_type_coercion() {
    let metadata = common::metadata_single_key("Order", "order_id");

    let mut rep_keys = HashMap::new();
    rep_keys.insert("order_id".to_string(), json!(789));
    let mut rep_all = HashMap::new();
    rep_all.insert("order_id".to_string(), json!(789));
    let rep = EntityRepresentation {
        typename:   "Order".to_string(),
        key_fields: rep_keys,
        all_fields: rep_all,
    };

    let result =
        construct_where_in_clause("Order", &[rep], &metadata, DatabaseType::PostgreSQL).unwrap();
    assert_eq!(result.sql, "order_id IN ($1)");
    // Numeric value converted to string in params
    assert_eq!(result.params, vec![json!("789")]);
}
