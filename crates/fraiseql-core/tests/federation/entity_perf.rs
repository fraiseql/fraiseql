//! Entity resolution at scale against **real PostgreSQL** — single, batch-100,
//! and repeated resolution over a reused pool.
//!
//! Micro-latency benchmarking lives in `benches/`; these assert correctness of
//! the parameterized `_entities` path against a real database.

#![allow(clippy::unwrap_used, clippy::panic, clippy::print_stderr)] // Reason: test code (skip notes to stderr)
use fraiseql_core::federation::{
    database_resolver::DatabaseEntityResolver, selection_parser::FieldSelection,
};
use serde_json::json;

use super::common;

fn id_name_selection() -> FieldSelection {
    FieldSelection::new(vec![
        "__typename".to_string(),
        "id".to_string(),
        "name".to_string(),
    ])
}

#[tokio::test]
async fn test_single_entity_resolution() {
    let rows = vec![common::row(&[
        ("id", json!("user1")),
        ("name", json!("John")),
    ])];
    let Some((_pg, adapter)) =
        common::pg_entity_fixture("user", &["id text", "name text"], &rows).await
    else {
        eprintln!("SKIP test_single_entity_resolution: no postgres (set DATABASE_URL)");
        return;
    };

    let resolver = DatabaseEntityResolver::new(adapter, common::metadata_single_key("User", "id"));
    let entities = resolver
        .resolve_entities_from_db(
            "User",
            &[common::rep("User", &[("id", json!("user1"))])],
            &id_name_selection(),
        )
        .await
        .unwrap_or_else(|e| panic!("resolve failed: {e}"));

    assert_eq!(entities.len(), 1);
    assert_eq!(entities[0].as_ref().expect("entity must resolve")["name"], "John");
}

#[tokio::test]
async fn test_batch_100_entities_resolution() {
    let mut rows = Vec::new();
    let mut reps = Vec::new();
    for i in 0..100 {
        let id = format!("user{i}");
        rows.push(common::row(&[("id", json!(id)), ("name", json!(format!("User {i}")))]));
        reps.push(common::rep("User", &[("id", json!(id))]));
    }
    let Some((_pg, adapter)) =
        common::pg_entity_fixture("user", &["id text", "name text"], &rows).await
    else {
        eprintln!("SKIP test_batch_100_entities_resolution: no postgres");
        return;
    };

    let resolver = DatabaseEntityResolver::new(adapter, common::metadata_single_key("User", "id"));
    let entities = resolver
        .resolve_entities_from_db("User", &reps, &id_name_selection())
        .await
        .unwrap_or_else(|e| panic!("resolve failed: {e}"));

    assert_eq!(entities.len(), 100);
    assert!(entities.iter().all(Option::is_some));
}

#[tokio::test]
async fn test_repeated_resolution_over_reused_pool() {
    let mut rows = Vec::new();
    for i in 0..5 {
        rows.push(common::row(&[
            ("id", json!(format!("user{i}"))),
            ("name", json!(format!("User {i}"))),
        ]));
    }
    let Some((_pg, adapter)) =
        common::pg_entity_fixture("user", &["id text", "name text"], &rows).await
    else {
        eprintln!("SKIP test_repeated_resolution_over_reused_pool: no postgres");
        return;
    };

    let metadata = common::metadata_single_key("User", "id");
    for i in 0..5 {
        let resolver = DatabaseEntityResolver::new(adapter.clone(), metadata.clone());
        let entities = resolver
            .resolve_entities_from_db(
                "User",
                &[common::rep("User", &[("id", json!(format!("user{i}")))])],
                &id_name_selection(),
            )
            .await
            .unwrap_or_else(|e| panic!("resolve failed on attempt {i}: {e}"));
        assert_eq!(entities.len(), 1);
        assert!(entities[0].is_some());
    }
}
