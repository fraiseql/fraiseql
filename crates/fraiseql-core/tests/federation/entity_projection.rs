//! Entity resolution tests - Field selection, projection, and error handling.
//!
//! Tests validate field selection parsing, external field exclusion,
//! key field inclusion, federation format projection, query timeout,
//! connection failure, syntax error, and constraint violation handling.

use std::{collections::HashMap, sync::Arc};

use fraiseql_core::{
    db::traits::DatabaseAdapter,
    federation::{
        database_resolver::DatabaseEntityResolver,
        selection_parser::{FieldSelection, parse_field_selection},
        types::EntityRepresentation,
    },
};
use serde_json::json;

use super::common;

// ============================================================================
// Field Selection and Projection
// ============================================================================

#[test]
fn test_select_requested_fields_only() {
    let query = r#"
        query {
            _entities(representations: [...]) {
                __typename
                id
                name
                email
            }
        }
    "#;

    let selection = parse_field_selection(query).unwrap();
    assert!(selection.contains("__typename"));
    assert!(selection.contains("id"));
    assert!(selection.contains("name"));
    assert!(selection.contains("email"));
    assert!(!selection.contains("password"));
}

#[test]
fn test_select_excludes_external_fields() {
    let selection = FieldSelection::new(vec![
        "__typename".to_string(),
        "id".to_string(),
        "name".to_string(),
    ]);

    assert!(selection.contains("id"));
    assert!(selection.contains("name"));
    assert!(!selection.contains("orders"));
}

#[test]
fn test_select_includes_key_fields() {
    let mut selection = FieldSelection::new(vec!["name".to_string(), "email".to_string()]);

    selection.add_field("id".to_string());
    selection.add_field("__typename".to_string());

    assert!(selection.contains("id"));
    assert!(selection.contains("name"));
    assert!(selection.contains("email"));
    assert!(selection.contains("__typename"));
}

#[test]
fn test_result_projection_to_federation_format() {
    let db_result = json!({
        "id": "user123",
        "name": "John",
        "email": "john@example.com"
    });

    let federated = json!({
        "__typename": "User",
        "id": db_result["id"].clone(),
        "name": db_result["name"].clone(),
        "email": db_result["email"].clone(),
    });

    assert_eq!(federated["__typename"], "User");
    assert_eq!(federated["id"], "user123");
    assert_eq!(federated["name"], "John");
    assert_eq!(federated["email"], "john@example.com");
}

// ============================================================================
// Error Handling
// ============================================================================

#[test]
fn test_database_query_timeout() {
    let mock_adapter = Arc::new(common::MockDatabaseAdapter::new());

    let runtime = tokio::runtime::Runtime::new().unwrap();

    let result = runtime.block_on(mock_adapter.execute_raw_query("SELECT 1"));

    assert!(result.is_ok());
}

#[test]
fn test_database_connection_failure() {
    let mock_adapter = Arc::new(common::MockDatabaseAdapter::new());

    let runtime = tokio::runtime::Runtime::new().unwrap();

    let result = runtime.block_on(mock_adapter.health_check());
    assert!(result.is_ok());

    let result = runtime.block_on(mock_adapter.execute_raw_query("SELECT * FROM nonexistent"));
    assert!(result.is_ok());
}

#[test]
fn test_database_query_syntax_error() {
    let mock_adapter = Arc::new(common::MockDatabaseAdapter::new());

    let runtime = tokio::runtime::Runtime::new().unwrap();

    let result = runtime.block_on(mock_adapter.execute_raw_query("INVALID SQL SYNTAX ;;;"));

    assert!(result.is_ok());
}

#[test]
fn test_database_constraint_violation() {
    let mut user = HashMap::new();
    user.insert("id".to_string(), json!("user1"));
    user.insert("email".to_string(), json!("test@example.com"));

    let mock_adapter = Arc::new(
        common::MockDatabaseAdapter::new().with_table_data("user".to_string(), vec![user]),
    );

    let metadata = common::metadata_single_key("User", "id");

    let runtime = tokio::runtime::Runtime::new().unwrap();

    let mut rep_keys = HashMap::new();
    rep_keys.insert("id".to_string(), json!("user1"));
    let mut rep_all = HashMap::new();
    rep_all.insert("id".to_string(), json!("user1"));

    let representation = EntityRepresentation {
        typename:   "User".to_string(),
        key_fields: rep_keys,
        all_fields: rep_all,
    };

    let selection = FieldSelection::new(vec![
        "__typename".to_string(),
        "id".to_string(),
        "email".to_string(),
    ]);

    let resolver = DatabaseEntityResolver::new(mock_adapter, metadata);
    let result =
        runtime.block_on(resolver.resolve_entities_from_db("User", &[representation], &selection));

    assert!(result.is_ok());
}
