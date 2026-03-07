//! Mutation type detection and variables.

#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable
use fraiseql_core::federation::{
    mutation_detector::{is_extended_mutation, is_local_mutation, is_mutation},
    mutation_query_builder::{build_delete_query, build_insert_query, build_update_query},
    types::FederationMetadata,
};
use serde_json::json;

use super::common;

#[test]
fn test_detect_mutation_query() {
    assert!(is_mutation("mutation { updateUser { id } }"));
    assert!(is_mutation("mutation UpdateUser { updateUser(id: \"123\") { id } }"));
    assert!(is_mutation("  mutation  {  createOrder  {  id  }  }"));
    assert!(!is_mutation("query { user { id } }"));
    assert!(!is_mutation("{ user { id } }"));
}

#[test]
fn test_detect_mutation_on_owned_entity() {
    let mutation_query = "mutation { updateUser { id } }";
    assert!(is_mutation(mutation_query));

    // Test with federation disabled - all mutations are local
    let metadata = FederationMetadata::default();
    assert!(is_local_mutation("updateUser", &metadata));
}

#[test]
fn test_detect_mutation_on_extended_entity() {
    let mutation_query = "mutation { updateOrder { id } }";
    assert!(is_mutation(mutation_query));

    // Test with federation disabled - no mutations are extended
    let metadata = FederationMetadata::default();
    assert!(!is_extended_mutation("updateUser", &metadata));
}

#[test]
fn test_mutation_with_variables() {
    let metadata = common::metadata_single_key("User", "id");

    let variables = json!({
        "id": "user123",
        "email": "test@example.com",
        "name": "Test User"
    });

    let update_query = build_update_query("User", &variables, &metadata).unwrap();
    assert!(update_query.contains("UPDATE user"));
    assert!(update_query.contains("SET"));
    assert!(update_query.contains("WHERE id = 'user123'"));

    let insert_query = build_insert_query("User", &variables, &metadata).unwrap();
    assert!(insert_query.contains("INSERT INTO user"));
    assert!(insert_query.contains("VALUES"));

    let delete_query = build_delete_query("User", &variables, &metadata).unwrap();
    assert!(delete_query.contains("DELETE FROM user"));
    assert!(delete_query.contains("WHERE id = 'user123'"));
}

#[test]
fn test_mutation_variable_validation() {
    let metadata = common::metadata_single_key("User", "id");

    // Missing key field should error
    let missing_key = json!({
        "email": "test@example.com"
    });

    let result = build_update_query("User", &missing_key, &metadata);
    assert!(result.is_err());
}

#[test]
fn test_mutation_input_type_coercion() {
    let metadata = common::metadata_single_key("Order", "order_id");

    let variables = json!({
        "order_id": 789,
        "total": 99.99,
        "active": true
    });

    let update_query = build_update_query("Order", &variables, &metadata).unwrap();
    // Numbers are not quoted in SQL (correctly)
    assert!(update_query.contains("WHERE order_id = 789"));
    assert!(update_query.contains("total = 99.99"));
    assert!(update_query.contains("active = true"));
}
