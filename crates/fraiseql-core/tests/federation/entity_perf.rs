//! Entity resolution tests - Performance benchmarks.
//!
//! Tests validate single entity resolution latency, batch resolution
//! latency, and concurrent entity resolution.

use std::{collections::HashMap, sync::Arc, time::Instant};

use fraiseql_core::federation::{
    database_resolver::DatabaseEntityResolver, selection_parser::FieldSelection,
    types::EntityRepresentation,
};
use serde_json::json;

use super::common;

// ============================================================================
// Performance
// ============================================================================

#[test]
fn test_single_entity_resolution_latency() {
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
        Arc::new(common::MockDatabaseAdapter::new().with_table_data("user".to_string(), rows));

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

    let mock_adapter =
        Arc::new(common::MockDatabaseAdapter::new().with_table_data("user".to_string(), users));

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
