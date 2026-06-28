//! Connection management and query execution against **real PostgreSQL** —
//! pool metrics, connection reuse, raw + parameterized query execution, and
//! real error handling (missing table).
//!
//! All tests provision via `common::pg_entity_fixture` / `common::pg_adapter`
//! and skip cleanly when no Postgres is configured (the non-DB preflight leg).

#![allow(clippy::unwrap_used, clippy::panic, clippy::print_stderr)] // Reason: test code (skip notes to stderr)
use fraiseql_core::{
    db::traits::DatabaseAdapter,
    federation::{database_resolver::DatabaseEntityResolver, selection_parser::FieldSelection},
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
async fn test_connection_pool_metrics_are_sane() {
    let Some((_pg, adapter)) = common::pg_adapter().await else {
        eprintln!("SKIP test_connection_pool_metrics_are_sane: no postgres (set DATABASE_URL)");
        return;
    };
    // Use a connection so the pool establishes at least one.
    adapter.execute_raw_query("SELECT 1").await.expect("trivial query");

    let metrics = adapter.pool_metrics();
    assert!(metrics.total_connections >= 1, "a query must establish a pooled connection");
    assert!(metrics.idle_connections <= metrics.total_connections);
    assert!(metrics.active_connections <= metrics.total_connections);
}

#[tokio::test]
async fn test_connection_reuse_across_resolutions() {
    let rows = vec![common::row(&[
        ("id", json!("user1")),
        ("name", json!("Bob")),
    ])];
    let Some((_pg, adapter)) =
        common::pg_entity_fixture("user", &["id text", "name text"], &rows).await
    else {
        eprintln!("SKIP test_connection_reuse_across_resolutions: no postgres");
        return;
    };

    let metadata = common::metadata_single_key("User", "id");
    for attempt in 0..3 {
        let resolver = DatabaseEntityResolver::new(adapter.clone(), metadata.clone());
        let entities = resolver
            .resolve_entities_from_db(
                "User",
                &[common::rep("User", &[("id", json!("user1"))])],
                &id_name_selection(),
            )
            .await
            .unwrap_or_else(|e| panic!("resolve failed on attempt {attempt}: {e}"));
        assert_eq!(entities.len(), 1);
        assert!(entities[0].is_some());
    }
}

#[tokio::test]
async fn test_query_execution_projects_selected_fields() {
    let rows = vec![
        common::row(&[
            ("id", json!("user1")),
            ("email", json!("user1@example.com")),
        ]),
        common::row(&[
            ("id", json!("user2")),
            ("email", json!("user2@example.com")),
        ]),
    ];
    let Some((_pg, adapter)) =
        common::pg_entity_fixture("user", &["id text", "email text"], &rows).await
    else {
        eprintln!("SKIP test_query_execution_projects_selected_fields: no postgres");
        return;
    };

    let metadata = common::metadata_single_key("User", "id");
    let reps = vec![
        common::rep("User", &[("id", json!("user1"))]),
        common::rep("User", &[("id", json!("user2"))]),
    ];
    let selection = FieldSelection::new(vec![
        "__typename".to_string(),
        "id".to_string(),
        "email".to_string(),
    ]);

    let resolver = DatabaseEntityResolver::new(adapter, metadata);
    let entities = resolver
        .resolve_entities_from_db("User", &reps, &selection)
        .await
        .unwrap_or_else(|e| panic!("resolve failed: {e}"));

    assert_eq!(entities.len(), 2);
    assert_eq!(entities[0].as_ref().expect("entity 0")["email"], "user1@example.com");
    assert_eq!(entities[1].as_ref().expect("entity 1")["email"], "user2@example.com");
}

#[tokio::test]
async fn test_raw_query_executes_against_real_db() {
    let rows = vec![common::row(&[
        ("id", json!("user1")),
        ("name", json!("John")),
    ])];
    let Some((_pg, adapter)) =
        common::pg_entity_fixture("user", &["id text", "name text"], &rows).await
    else {
        eprintln!("SKIP test_raw_query_executes_against_real_db: no postgres");
        return;
    };

    let result = adapter
        .execute_raw_query(r#"SELECT id, name FROM "user" WHERE id = 'user1'"#)
        .await
        .expect("raw query must execute");
    assert_eq!(result.len(), 1);
    assert_eq!(result[0]["id"], "user1");
    assert_eq!(result[0]["name"], "John");
}

#[tokio::test]
async fn test_query_on_missing_table_errors() {
    // The mock faked success for any SQL; a real adapter must surface the error
    // for a nonexistent relation.
    let Some((_pg, adapter)) = common::pg_adapter().await else {
        eprintln!("SKIP test_query_on_missing_table_errors: no postgres");
        return;
    };
    let result = adapter
        .execute_raw_query(r#"SELECT * FROM "definitely_not_a_real_table""#)
        .await;
    assert!(result.is_err(), "querying a nonexistent table must return an error");
}

#[test]
fn test_database_parameterized_queries() {
    use fraiseql_core::federation::{
        query_builder::construct_where_in_clause, types::EntityRepresentation,
    };
    use fraiseql_db::DatabaseType;

    let metadata = common::metadata_single_key("User", "id");
    let representation = EntityRepresentation {
        typename:   "User".to_string(),
        key_fields: common::row(&[("id", json!("O'Brien"))]),
        all_fields: common::row(&[("id", json!("O'Brien"))]),
    };

    let (where_clause, params) =
        construct_where_in_clause("User", &[representation], &metadata, DatabaseType::PostgreSQL)
            .unwrap();

    // The value is bound as a parameter, not escaped into the SQL text.
    // Key column cast to text on PostgreSQL (#504).
    assert_eq!(where_clause, "id::text IN ($1)");
    assert_eq!(params, vec![json!("O'Brien")]);
}
