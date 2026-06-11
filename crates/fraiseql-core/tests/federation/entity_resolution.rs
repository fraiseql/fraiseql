//! Entity resolution tests — single, batch, composite key, null handling, and
//! large-result-set resolution against **real PostgreSQL**.
//!
//! Each test provisions a real table via `common::pg_entity_fixture` and skips
//! cleanly when no Postgres is configured (the non-DB preflight leg); they run
//! for real on the Dagger `integration --suite=postgres` leg, where the
//! federation test target is invoked with a bound `DATABASE_URL`. This exercises
//! the parameterized `_entities` SQL path end-to-end (H3).

#![allow(clippy::unwrap_used, clippy::panic, clippy::print_stderr)] // Reason: test code (skip notes to stderr)
use std::collections::HashMap;

use fraiseql_core::federation::{
    database_resolver::DatabaseEntityResolver, selection_parser::FieldSelection,
    types::EntityRepresentation,
};
use serde_json::{Value, json};

use super::common;

/// Build a row / key map from (column, value) pairs.
fn map(pairs: &[(&str, Value)]) -> HashMap<String, Value> {
    pairs.iter().map(|(k, v)| ((*k).to_string(), v.clone())).collect()
}

/// Build a representation for `typename` from its key (column, value) pairs.
fn rep(typename: &str, keys: &[(&str, Value)]) -> EntityRepresentation {
    let key_fields = map(keys);
    EntityRepresentation {
        typename: typename.to_string(),
        all_fields: key_fields.clone(),
        key_fields,
    }
}

#[tokio::test]
async fn test_resolve_entity_from_postgres_table() {
    let rows = vec![map(&[
        ("id", json!("user123")),
        ("name", json!("John Doe")),
        ("email", json!("john@example.com")),
    ])];
    let Some((_pg, adapter)) =
        common::pg_entity_fixture("user", &["id text", "name text", "email text"], &rows).await
    else {
        eprintln!("SKIP test_resolve_entity_from_postgres_table: no postgres (set DATABASE_URL)");
        return;
    };

    let metadata = common::metadata_single_key("User", "id");
    let representation = rep("User", &[("id", json!("user123"))]);
    let selection = FieldSelection::new(vec![
        "__typename".to_string(),
        "id".to_string(),
        "name".to_string(),
        "email".to_string(),
    ]);

    let resolver = DatabaseEntityResolver::new(adapter, metadata);
    let entities = resolver
        .resolve_entities_from_db("User", &[representation], &selection)
        .await
        .unwrap_or_else(|e| panic!("resolve_entities_from_db (postgres table) failed: {e}"));

    assert_eq!(entities.len(), 1);
    let entity = entities[0].as_ref().expect("entity must resolve");
    assert_eq!(entity["__typename"], "User");
    assert_eq!(entity["id"], "user123");
    assert_eq!(entity["name"], "John Doe");
}

#[tokio::test]
async fn test_resolve_entities_batch_from_postgres() {
    let rows = vec![
        map(&[("id", json!("user1")), ("name", json!("Alice"))]),
        map(&[("id", json!("user2")), ("name", json!("Bob"))]),
    ];
    let Some((_pg, adapter)) =
        common::pg_entity_fixture("user", &["id text", "name text"], &rows).await
    else {
        eprintln!("SKIP test_resolve_entities_batch_from_postgres: no postgres (set DATABASE_URL)");
        return;
    };

    let metadata = common::metadata_single_key("User", "id");
    let reps = vec![
        rep("User", &[("id", json!("user1"))]),
        rep("User", &[("id", json!("user2"))]),
    ];
    let selection = FieldSelection::new(vec![
        "__typename".to_string(),
        "id".to_string(),
        "name".to_string(),
    ]);

    let resolver = DatabaseEntityResolver::new(adapter, metadata);
    let entities = resolver
        .resolve_entities_from_db("User", &reps, &selection)
        .await
        .unwrap_or_else(|e| panic!("resolve_entities_from_db (batch) failed: {e}"));

    assert_eq!(entities.len(), 2);
    // project_results preserves representation order, independent of DB row order.
    assert_eq!(entities[0].as_ref().expect("entity 0")["name"], "Alice");
    assert_eq!(entities[1].as_ref().expect("entity 1")["name"], "Bob");
}

#[tokio::test]
async fn test_resolve_entity_composite_key_from_postgres() {
    let rows = vec![map(&[
        ("tenant_id", json!("t1")),
        ("user_id", json!("u1")),
        ("name", json!("John")),
    ])];
    let Some((_pg, adapter)) =
        common::pg_entity_fixture("user", &["tenant_id text", "user_id text", "name text"], &rows)
            .await
    else {
        eprintln!("SKIP test_resolve_entity_composite_key_from_postgres: no postgres");
        return;
    };

    let metadata = common::metadata_composite_key("User", &["tenant_id", "user_id"]);
    let representation = rep("User", &[("tenant_id", json!("t1")), ("user_id", json!("u1"))]);
    let selection = FieldSelection::new(vec![
        "__typename".to_string(),
        "tenant_id".to_string(),
        "user_id".to_string(),
        "name".to_string(),
    ]);

    let resolver = DatabaseEntityResolver::new(adapter, metadata);
    let entities = resolver
        .resolve_entities_from_db("User", &[representation], &selection)
        .await
        .unwrap_or_else(|e| panic!("resolve_entities_from_db (composite key) failed: {e}"));

    assert_eq!(entities.len(), 1);
    assert_eq!(entities[0].as_ref().expect("entity must resolve")["name"], "John");
}

#[tokio::test]
async fn test_resolve_entity_with_null_values_from_postgres() {
    let rows = vec![map(&[
        ("id", json!("user123")),
        ("name", json!("John")),
        ("email", Value::Null),
    ])];
    let Some((_pg, adapter)) =
        common::pg_entity_fixture("user", &["id text", "name text", "email text"], &rows).await
    else {
        eprintln!("SKIP test_resolve_entity_with_null_values_from_postgres: no postgres");
        return;
    };

    let metadata = common::metadata_single_key("User", "id");
    let representation = rep("User", &[("id", json!("user123"))]);
    let selection = FieldSelection::new(vec![
        "__typename".to_string(),
        "id".to_string(),
        "name".to_string(),
        "email".to_string(),
    ]);

    let resolver = DatabaseEntityResolver::new(adapter, metadata);
    let entities = resolver
        .resolve_entities_from_db("User", &[representation], &selection)
        .await
        .unwrap_or_else(|e| panic!("resolve_entities_from_db (null values) failed: {e}"));

    assert_eq!(entities.len(), 1);
    let entity = entities[0].as_ref().expect("entity must resolve");
    assert_eq!(entity["name"], "John");
    assert_eq!(entity["email"], Value::Null);
}

#[tokio::test]
async fn test_resolve_entity_large_result_set_from_postgres() {
    let mut rows = Vec::new();
    let mut reps = Vec::new();
    for i in 0..100 {
        let id = format!("user{i}");
        rows.push(map(&[("id", json!(id)), ("name", json!(format!("User {i}")))]));
        reps.push(rep("User", &[("id", json!(id))]));
    }
    let Some((_pg, adapter)) =
        common::pg_entity_fixture("user", &["id text", "name text"], &rows).await
    else {
        eprintln!("SKIP test_resolve_entity_large_result_set_from_postgres: no postgres");
        return;
    };

    let metadata = common::metadata_single_key("User", "id");
    let selection = FieldSelection::new(vec![
        "__typename".to_string(),
        "id".to_string(),
        "name".to_string(),
    ]);

    let resolver = DatabaseEntityResolver::new(adapter, metadata);
    let entities = resolver
        .resolve_entities_from_db("User", &reps, &selection)
        .await
        .unwrap_or_else(|e| panic!("resolve_entities_from_db (large result set) failed: {e}"));

    assert_eq!(entities.len(), 100);
    for entity in &entities {
        assert!(entity.is_some());
    }
}
