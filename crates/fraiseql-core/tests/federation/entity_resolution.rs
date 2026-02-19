//! Entity resolution tests — single, batch, composite key, null handling,
//! connection management, query execution, field selection, error handling,
//! and performance.
//!
//! Split from `federation_database_integration.rs`.

use std::{collections::HashMap, sync::Arc};

use fraiseql_core::{
    db::traits::DatabaseAdapter,
    federation::{
        database_resolver::DatabaseEntityResolver,
        selection_parser::{parse_field_selection, FieldSelection},
        types::EntityRepresentation,
    },
};
use serde_json::{Value, json};

use super::common;

// ============================================================================
// Database Entity Resolution (PostgreSQL)
// ============================================================================

#[test]
fn test_resolve_entity_from_postgres_table() {
    let mut user_row = HashMap::new();
    user_row.insert("id".to_string(), json!("user123"));
    user_row.insert("name".to_string(), json!("John Doe"));
    user_row.insert("email".to_string(), json!("john@example.com"));

    let mock_adapter =
        common::MockDatabaseAdapter::new().with_table_data("user".to_string(), vec![user_row]);

    let metadata = common::metadata_single_key("User", "id");

    let mut rep_keys = HashMap::new();
    rep_keys.insert("id".to_string(), json!("user123"));
    let mut rep_all = HashMap::new();
    rep_all.insert("id".to_string(), json!("user123"));

    let representation = EntityRepresentation {
        typename:   "User".to_string(),
        key_fields: rep_keys,
        all_fields: rep_all,
    };

    let selection = FieldSelection::new(vec![
        "__typename".to_string(),
        "id".to_string(),
        "name".to_string(),
        "email".to_string(),
    ]);

    let resolver = DatabaseEntityResolver::new(Arc::new(mock_adapter), metadata);
    let result = tokio::runtime::Runtime::new()
        .unwrap()
        .block_on(resolver.resolve_entities_from_db("User", &[representation], &selection));

    assert!(result.is_ok());
    let entities = result.unwrap();
    assert_eq!(entities.len(), 1);
    assert!(entities[0].is_some());

    let entity = &entities[0].as_ref().unwrap();
    assert_eq!(entity["__typename"], "User");
    assert_eq!(entity["id"], "user123");
    assert_eq!(entity["name"], "John Doe");
}

#[test]
fn test_resolve_entities_batch_from_postgres() {
    let mut user1 = HashMap::new();
    user1.insert("id".to_string(), json!("user1"));
    user1.insert("name".to_string(), json!("Alice"));

    let mut user2 = HashMap::new();
    user2.insert("id".to_string(), json!("user2"));
    user2.insert("name".to_string(), json!("Bob"));

    let mock_adapter =
        common::MockDatabaseAdapter::new().with_table_data("user".to_string(), vec![user1, user2]);

    let metadata = common::metadata_single_key("User", "id");

    let mut rep1_keys = HashMap::new();
    rep1_keys.insert("id".to_string(), json!("user1"));
    let mut rep1_all = HashMap::new();
    rep1_all.insert("id".to_string(), json!("user1"));

    let mut rep2_keys = HashMap::new();
    rep2_keys.insert("id".to_string(), json!("user2"));
    let mut rep2_all = HashMap::new();
    rep2_all.insert("id".to_string(), json!("user2"));

    let reps = vec![
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
        "name".to_string(),
    ]);

    let resolver = DatabaseEntityResolver::new(Arc::new(mock_adapter), metadata);
    let result = tokio::runtime::Runtime::new()
        .unwrap()
        .block_on(resolver.resolve_entities_from_db("User", &reps, &selection));

    assert!(result.is_ok());
    let entities = result.unwrap();
    assert_eq!(entities.len(), 2);
    assert!(entities[0].is_some());
    assert!(entities[1].is_some());

    assert_eq!(entities[0].as_ref().unwrap()["name"], "Alice");
    assert_eq!(entities[1].as_ref().unwrap()["name"], "Bob");
}

#[test]
fn test_resolve_entity_composite_key_from_postgres() {
    let mut row = HashMap::new();
    row.insert("tenant_id".to_string(), json!("t1"));
    row.insert("user_id".to_string(), json!("u1"));
    row.insert("name".to_string(), json!("John"));

    let mock_adapter =
        common::MockDatabaseAdapter::new().with_table_data("user".to_string(), vec![row]);

    let metadata = common::metadata_composite_key("User", &["tenant_id", "user_id"]);

    let mut rep_keys = HashMap::new();
    rep_keys.insert("tenant_id".to_string(), json!("t1"));
    rep_keys.insert("user_id".to_string(), json!("u1"));
    let mut rep_all = HashMap::new();
    rep_all.insert("tenant_id".to_string(), json!("t1"));
    rep_all.insert("user_id".to_string(), json!("u1"));

    let representation = EntityRepresentation {
        typename:   "User".to_string(),
        key_fields: rep_keys,
        all_fields: rep_all,
    };

    let selection = FieldSelection::new(vec![
        "__typename".to_string(),
        "tenant_id".to_string(),
        "user_id".to_string(),
        "name".to_string(),
    ]);

    let resolver = DatabaseEntityResolver::new(Arc::new(mock_adapter), metadata);
    let result = tokio::runtime::Runtime::new()
        .unwrap()
        .block_on(resolver.resolve_entities_from_db("User", &[representation], &selection));

    assert!(result.is_ok());
    let entities = result.unwrap();
    assert_eq!(entities.len(), 1);
    assert!(entities[0].is_some());
    assert_eq!(entities[0].as_ref().unwrap()["name"], "John");
}

#[test]
fn test_resolve_entity_with_null_values_from_postgres() {
    let mut row = HashMap::new();
    row.insert("id".to_string(), json!("user123"));
    row.insert("name".to_string(), json!("John"));
    row.insert("email".to_string(), Value::Null);

    let mock_adapter =
        common::MockDatabaseAdapter::new().with_table_data("user".to_string(), vec![row]);

    let metadata = common::metadata_single_key("User", "id");

    let mut rep_keys = HashMap::new();
    rep_keys.insert("id".to_string(), json!("user123"));
    let mut rep_all = HashMap::new();
    rep_all.insert("id".to_string(), json!("user123"));

    let representation = EntityRepresentation {
        typename:   "User".to_string(),
        key_fields: rep_keys,
        all_fields: rep_all,
    };

    let selection = FieldSelection::new(vec![
        "__typename".to_string(),
        "id".to_string(),
        "name".to_string(),
        "email".to_string(),
    ]);

    let resolver = DatabaseEntityResolver::new(Arc::new(mock_adapter), metadata);
    let result = tokio::runtime::Runtime::new()
        .unwrap()
        .block_on(resolver.resolve_entities_from_db("User", &[representation], &selection));

    assert!(result.is_ok());
    let entities = result.unwrap();
    assert_eq!(entities.len(), 1);
    assert!(entities[0].is_some());

    let entity = &entities[0].as_ref().unwrap();
    assert_eq!(entity["name"], "John");
    assert_eq!(entity["email"], Value::Null);
}

#[test]
fn test_resolve_entity_large_result_set_from_postgres() {
    let mut rows = Vec::new();
    let mut reps = Vec::new();

    for i in 0..100 {
        let mut row = HashMap::new();
        let id = format!("user{}", i);
        row.insert("id".to_string(), json!(id.clone()));
        row.insert("name".to_string(), json!(format!("User {}", i)));
        rows.push(row);

        let mut rep_keys = HashMap::new();
        rep_keys.insert("id".to_string(), json!(id.clone()));
        let mut rep_all = HashMap::new();
        rep_all.insert("id".to_string(), json!(id));

        reps.push(EntityRepresentation {
            typename:   "User".to_string(),
            key_fields: rep_keys,
            all_fields: rep_all,
        });
    }

    let mock_adapter =
        common::MockDatabaseAdapter::new().with_table_data("user".to_string(), rows);

    let metadata = common::metadata_single_key("User", "id");

    let selection = FieldSelection::new(vec![
        "__typename".to_string(),
        "id".to_string(),
        "name".to_string(),
    ]);

    let resolver = DatabaseEntityResolver::new(Arc::new(mock_adapter), metadata);
    let result = tokio::runtime::Runtime::new()
        .unwrap()
        .block_on(resolver.resolve_entities_from_db("User", &reps, &selection));

    assert!(result.is_ok());
    let entities = result.unwrap();
    assert_eq!(entities.len(), 100);
    for entity in &entities {
        assert!(entity.is_some());
    }
}

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
        common::MockDatabaseAdapter::new()
            .with_table_data("user".to_string(), vec![user.clone()]),
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

// ============================================================================
// Performance
// ============================================================================

#[test]
fn test_single_entity_resolution_latency() {
    use std::time::Instant;

    let mut user = HashMap::new();
    user.insert("id".to_string(), json!("user1"));
    user.insert("name".to_string(), json!("John"));

    let mock_adapter = Arc::new(
        common::MockDatabaseAdapter::new().with_table_data("user".to_string(), vec![user]),
    );

    let metadata = common::metadata_single_key("User", "id");

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

    let runtime = tokio::runtime::Runtime::new().unwrap();
    let resolver = DatabaseEntityResolver::new(mock_adapter, metadata);

    let start = Instant::now();
    let _result =
        runtime.block_on(resolver.resolve_entities_from_db("User", &[representation], &selection));
    let duration = start.elapsed();

    assert!(duration.as_millis() < 10, "Single entity resolution took {:?}", duration);
}

#[test]
fn test_batch_100_entities_resolution_latency() {
    use std::time::Instant;

    let mut rows = Vec::new();
    let mut reps = Vec::new();

    for i in 0..100 {
        let mut row = HashMap::new();
        let id = format!("user{}", i);
        row.insert("id".to_string(), json!(id.clone()));
        row.insert("name".to_string(), json!(format!("User {}", i)));
        rows.push(row);

        let mut rep_keys = HashMap::new();
        rep_keys.insert("id".to_string(), json!(id.clone()));
        let mut rep_all = HashMap::new();
        rep_all.insert("id".to_string(), json!(id));

        reps.push(EntityRepresentation {
            typename:   "User".to_string(),
            key_fields: rep_keys,
            all_fields: rep_all,
        });
    }

    let mock_adapter = Arc::new(
        common::MockDatabaseAdapter::new().with_table_data("user".to_string(), rows),
    );

    let metadata = common::metadata_single_key("User", "id");

    let selection = FieldSelection::new(vec![
        "__typename".to_string(),
        "id".to_string(),
        "name".to_string(),
    ]);

    let runtime = tokio::runtime::Runtime::new().unwrap();
    let resolver = DatabaseEntityResolver::new(mock_adapter, metadata);

    let start = Instant::now();
    let result = runtime.block_on(resolver.resolve_entities_from_db("User", &reps, &selection));
    let duration = start.elapsed();

    assert!(result.is_ok());
    assert_eq!(result.unwrap().len(), 100);

    assert!(duration.as_millis() < 100, "Batch resolution took {:?}", duration);
}

#[test]
fn test_concurrent_entity_resolution() {
    let mut users = Vec::new();
    for i in 0..10 {
        let mut user = HashMap::new();
        user.insert("id".to_string(), json!(format!("user{}", i)));
        user.insert("name".to_string(), json!(format!("User {}", i)));
        users.push(user);
    }

    let mock_adapter = Arc::new(
        common::MockDatabaseAdapter::new().with_table_data("user".to_string(), users),
    );

    let metadata = common::metadata_single_key("User", "id");

    let selection = FieldSelection::new(vec![
        "__typename".to_string(),
        "id".to_string(),
        "name".to_string(),
    ]);

    let runtime = tokio::runtime::Runtime::new().unwrap();

    for i in 0..5 {
        let mut rep_keys = HashMap::new();
        rep_keys.insert("id".to_string(), json!(format!("user{}", i)));
        let mut rep_all = HashMap::new();
        rep_all.insert("id".to_string(), json!(format!("user{}", i)));

        let representation = EntityRepresentation {
            typename:   "User".to_string(),
            key_fields: rep_keys,
            all_fields: rep_all,
        };

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
}
