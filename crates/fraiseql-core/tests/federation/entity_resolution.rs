//! Entity resolution tests — single, batch, composite key, null handling,
//! and large result set resolution from PostgreSQL.
//!
//! Split from `federation_database_integration.rs`.

use std::{collections::HashMap, sync::Arc};

use fraiseql_core::federation::{
    database_resolver::DatabaseEntityResolver, selection_parser::FieldSelection,
    types::EntityRepresentation,
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

    let mock_adapter = common::MockDatabaseAdapter::new().with_table_data("user".to_string(), rows);

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
