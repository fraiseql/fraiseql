//! Field selection / projection logic (pure) and query execution + error
//! handling against **real PostgreSQL**.

#![allow(clippy::unwrap_used, clippy::panic, clippy::print_stderr)] // Reason: test code (skip notes to stderr)
use fraiseql_core::{
    db::traits::DatabaseAdapter,
    federation::{
        database_resolver::DatabaseEntityResolver,
        selection_parser::{FieldSelection, parse_field_selection},
    },
};
use serde_json::json;

use super::common;

// ============================================================================
// Field Selection and Projection (pure)
// ============================================================================

#[test]
fn test_select_requested_fields_only() {
    let query = r"
        query {
            _entities(representations: [...]) {
                __typename
                id
                name
                email
            }
        }
    ";

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
// Query Execution and Error Handling (real PostgreSQL)
// ============================================================================

#[tokio::test]
async fn test_trivial_query_executes() {
    let Some((_pg, adapter)) = common::pg_adapter().await else {
        eprintln!("SKIP test_trivial_query_executes: no postgres (set DATABASE_URL)");
        return;
    };
    adapter.execute_raw_query("SELECT 1").await.expect("SELECT 1 must execute");
}

#[tokio::test]
async fn test_health_check_succeeds() {
    let Some((_pg, adapter)) = common::pg_adapter().await else {
        eprintln!("SKIP test_health_check_succeeds: no postgres");
        return;
    };
    adapter
        .health_check()
        .await
        .expect("health check must succeed against a live database");
}

#[tokio::test]
async fn test_invalid_sql_returns_error() {
    // The mock faked success for any SQL; a real adapter surfaces the parse error.
    let Some((_pg, adapter)) = common::pg_adapter().await else {
        eprintln!("SKIP test_invalid_sql_returns_error: no postgres");
        return;
    };
    let result = adapter.execute_raw_query("INVALID SQL SYNTAX ;;;").await;
    assert!(result.is_err(), "invalid SQL must return an error against a real database");
}

#[tokio::test]
async fn test_resolution_against_real_table() {
    let rows = vec![common::row(&[
        ("id", json!("user1")),
        ("email", json!("test@example.com")),
    ])];
    let Some((_pg, adapter)) =
        common::pg_entity_fixture("user", &["id text", "email text"], &rows).await
    else {
        eprintln!("SKIP test_resolution_against_real_table: no postgres");
        return;
    };

    let resolver = DatabaseEntityResolver::new(adapter, common::metadata_single_key("User", "id"));
    let entities = resolver
        .resolve_entities_from_db(
            "User",
            &[common::rep("User", &[("id", json!("user1"))])],
            &FieldSelection::new(vec![
                "__typename".to_string(),
                "id".to_string(),
                "email".to_string(),
            ]),
        )
        .await
        .unwrap_or_else(|e| panic!("resolve failed: {e}"));

    assert_eq!(entities.len(), 1);
    assert_eq!(entities[0].as_ref().expect("entity")["email"], "test@example.com");
}
