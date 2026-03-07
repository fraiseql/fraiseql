//! WHERE clause construction and SQL injection prevention tests.
//!
//! Split from `federation_database_integration.rs`.

#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable
#![allow(clippy::cast_possible_truncation)] // Reason: test uses usize→u32 for small test counts
use std::collections::HashMap;

use fraiseql_core::federation::{
    query_builder::construct_where_in_clause, types::EntityRepresentation,
};
use serde_json::json;

use super::common;

// ============================================================================
// WHERE Clause Construction
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

    let where_clause = construct_where_in_clause("User", &[rep1, rep2], &metadata).unwrap();
    assert_eq!(where_clause, "id IN ('123', '456')");
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

    let where_clause = construct_where_in_clause("Order", &[rep1], &metadata).unwrap();
    assert_eq!(where_clause, "(user_id, order_id) IN (('user1', 'order1'))");
}

#[test]
fn test_where_clause_string_escaping() {
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

    let where_clause = construct_where_in_clause("User", &[rep], &metadata).unwrap();
    assert_eq!(where_clause, "name IN ('O''Brien')");
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

    let where_clause = construct_where_in_clause("User", &[rep], &metadata).unwrap();
    assert_eq!(where_clause, "id IN ('''; DROP TABLE users; --')");
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

    let where_clause = construct_where_in_clause("Order", &[rep], &metadata).unwrap();
    assert_eq!(where_clause, "order_id IN ('789')");
}
