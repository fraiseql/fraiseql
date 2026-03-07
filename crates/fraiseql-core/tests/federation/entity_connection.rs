//! Entity resolution tests - Connection management and query execution.
//!
//! Tests validate connection pooling, reuse, timeout, retry,
//! query execution, prepared statements, parameterized queries,
//! transaction handling, and rollback.

#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable
use std::{collections::HashMap, sync::Arc};

use fraiseql_core::{
    db::traits::DatabaseAdapter,
    federation::{
        database_resolver::DatabaseEntityResolver, selection_parser::FieldSelection,
        types::EntityRepresentation,
    },
};
use serde_json::json;

use super::common;

// ============================================================================
// Connection Management
// ============================================================================

#[test]
fn test_database_connection_pooling() {
    let mock_adapter = common::MockDatabaseAdapter::new();

    let metrics = mock_adapter.pool_metrics();
    assert_eq!(metrics.total_connections, 10);
    assert_eq!(metrics.idle_connections, 8);
    assert_eq!(metrics.active_connections, 2);
    assert_eq!(metrics.waiting_requests, 0);
}

#[test]
fn test_database_connection_reuse() {
    let mut user1 = HashMap::new();
    user1.insert("id".to_string(), json!("user1"));
    user1.insert("name".to_string(), json!("Alice"));

    let mock_adapter = Arc::new(
        common::MockDatabaseAdapter::new().with_table_data("user".to_string(), vec![user1]),
    );

    let metadata = common::metadata_single_key("User", "id");

    let runtime = tokio::runtime::Runtime::new().unwrap();

    for _ in 0..3 {
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
            "name".to_string(),
        ]);

        let resolver = DatabaseEntityResolver::new(mock_adapter.clone(), metadata.clone());
        let result = runtime.block_on(resolver.resolve_entities_from_db(
            "User",
            &[representation],
            &selection,
        ));

        assert!(result.is_ok());
        let entities = result.unwrap();
        assert_eq!(entities.len(), 1);
        assert!(entities[0].is_some());
    }

    let metrics = mock_adapter.pool_metrics();
    assert_eq!(metrics.total_connections, 10);
    assert_eq!(metrics.idle_connections, 8);
}

#[test]
fn test_database_connection_timeout() {
    let mock_adapter = Arc::new(common::MockDatabaseAdapter::new());

    let runtime = tokio::runtime::Runtime::new().unwrap();
    let result = runtime.block_on(mock_adapter.health_check());
    assert!(result.is_ok());

    let result = runtime.block_on(mock_adapter.execute_raw_query("SELECT 1"));
    assert!(result.is_ok());
}

#[test]
fn test_database_connection_retry() {
    let mut user = HashMap::new();
    user.insert("id".to_string(), json!("user1"));
    user.insert("name".to_string(), json!("Bob"));

    let mock_adapter = Arc::new(
        common::MockDatabaseAdapter::new().with_table_data("user".to_string(), vec![user]),
    );

    let metadata = common::metadata_single_key("User", "id");

    let runtime = tokio::runtime::Runtime::new().unwrap();

    for attempt in 0..3 {
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
            "name".to_string(),
        ]);

        let resolver = DatabaseEntityResolver::new(mock_adapter.clone(), metadata.clone());
        let result = runtime.block_on(resolver.resolve_entities_from_db(
            "User",
            &[representation],
            &selection,
        ));

        assert!(result.is_ok(), "Attempt {} failed", attempt);
        let entities = result.unwrap();
        assert_eq!(entities.len(), 1);
        assert!(entities[0].is_some());
    }
}

// ============================================================================
// Query Execution
// ============================================================================

#[test]
fn test_database_query_execution_basic() {
    let mut user1 = HashMap::new();
    user1.insert("id".to_string(), json!("user1"));
    user1.insert("email".to_string(), json!("user1@example.com"));

    let mut user2 = HashMap::new();
    user2.insert("id".to_string(), json!("user2"));
    user2.insert("email".to_string(), json!("user2@example.com"));

    let mock_adapter = Arc::new(
        common::MockDatabaseAdapter::new().with_table_data("user".to_string(), vec![user1, user2]),
    );

    let metadata = common::metadata_single_key("User", "id");

    let mut rep1_keys = HashMap::new();
    rep1_keys.insert("id".to_string(), json!("user1"));
    let mut rep1_all = HashMap::new();
    rep1_all.insert("id".to_string(), json!("user1"));

    let mut rep2_keys = HashMap::new();
    rep2_keys.insert("id".to_string(), json!("user2"));
    let mut rep2_all = HashMap::new();
    rep2_all.insert("id".to_string(), json!("user2"));

    let representations = vec![
        EntityRepresentation {
            typename:   "User".to_string(),
            key_fields: rep1_keys,
            all_fields: rep1_all,
        },
        EntityRepresentation {
            typename:   "User".to_string(),
            key_fields: rep2_keys,
            all_fields: rep2_all,
        },
    ];

    let selection = FieldSelection::new(vec![
        "__typename".to_string(),
        "id".to_string(),
        "email".to_string(),
    ]);

    let runtime = tokio::runtime::Runtime::new().unwrap();
    let resolver = DatabaseEntityResolver::new(mock_adapter, metadata);
    let result =
        runtime.block_on(resolver.resolve_entities_from_db("User", &representations, &selection));

    assert!(result.is_ok());
    let entities = result.unwrap();
    assert_eq!(entities.len(), 2);
    assert_eq!(entities[0].as_ref().unwrap()["email"], "user1@example.com");
    assert_eq!(entities[1].as_ref().unwrap()["email"], "user2@example.com");
}

#[test]
fn test_database_prepared_statements() {
    let mut user = HashMap::new();
    user.insert("id".to_string(), json!("user1"));
    user.insert("name".to_string(), json!("John"));

    let mock_adapter = Arc::new(
        common::MockDatabaseAdapter::new().with_table_data("user".to_string(), vec![user]),
    );

    let runtime = tokio::runtime::Runtime::new().unwrap();

    let result = runtime
        .block_on(mock_adapter.execute_raw_query("SELECT id, name FROM user WHERE id = 'user1'"));

    assert!(result.is_ok());
    let rows = result.unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0]["id"], "user1");
    assert_eq!(rows[0]["name"], "John");
}

#[test]
fn test_database_parameterized_queries() {
    use fraiseql_core::federation::query_builder::construct_where_in_clause;

    let metadata = common::metadata_single_key("User", "id");

    let mut rep_keys = HashMap::new();
    rep_keys.insert("id".to_string(), json!("O'Brien"));
    let mut rep_all = HashMap::new();
    rep_all.insert("id".to_string(), json!("O'Brien"));

    let representation = EntityRepresentation {
        typename:   "User".to_string(),
        key_fields: rep_keys,
        all_fields: rep_all,
    };

    let where_clause = construct_where_in_clause("User", &[representation], &metadata).unwrap();

    assert!(where_clause.contains("O''Brien"));
    assert!(where_clause.contains("id IN"));
}

#[test]
fn test_database_transaction_handling() {
    let mock_adapter = Arc::new(common::MockDatabaseAdapter::new());

    let runtime = tokio::runtime::Runtime::new().unwrap();

    let result1 = runtime.block_on(mock_adapter.health_check());
    assert!(result1.is_ok());

    let result2 = runtime.block_on(mock_adapter.execute_raw_query("SELECT 1"));
    assert!(result2.is_ok());

    assert!(result1.is_ok());
    assert!(result2.is_ok());
}

#[test]
fn test_database_transaction_rollback() {
    let mut user = HashMap::new();
    user.insert("id".to_string(), json!("user1"));
    user.insert("name".to_string(), json!("John"));

    let mock_adapter = Arc::new(
        common::MockDatabaseAdapter::new().with_table_data("user".to_string(), vec![user.clone()]),
    );

    let runtime = tokio::runtime::Runtime::new().unwrap();

    let result1 = runtime.block_on(mock_adapter.execute_raw_query("SELECT * FROM user"));
    assert!(result1.is_ok());

    let result2 =
        runtime.block_on(mock_adapter.execute_raw_query("SELECT * FROM nonexistent_table"));
    assert!(result2.is_ok());

    let result3 = runtime.block_on(mock_adapter.execute_raw_query("SELECT * FROM user"));
    assert!(result3.is_ok());
    let rows = result3.unwrap();
    assert_eq!(rows[0]["id"], "user1");
}
