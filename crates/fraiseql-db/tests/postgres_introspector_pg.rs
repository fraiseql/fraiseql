//! PostgreSQL introspector integration tests.
//!
//! In `tests/`, where `cargo test --lib` cannot reach them (#1370).
//!
//! **Execution engine:** `PostgreSQL` · **Infrastructure:** `DATABASE_URL`.

#![cfg(all(feature = "postgres", feature = "test-postgres"))]
#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

use deadpool_postgres::{Config, ManagerConfig, RecyclingMethod, Runtime};
use fraiseql_db::{DatabaseIntrospector as _, DatabaseType, postgres::*};
use tokio_postgres::NoTls;

// Test DB URL from the `fraiseql_test_support` env-URL harness (`DATABASE_URL`), so this
// suite runs against a Dagger-bound service (local == CI) instead of a hardcoded host.
fn test_db_url() -> String {
    fraiseql_test_support::database_url()
}

// Helper to create test introspector
async fn create_test_introspector() -> PostgresIntrospector {
    let _adapter = PostgresAdapter::new(&test_db_url())
        .await
        .expect("Failed to create test adapter");

    // Extract pool from adapter (we need a way to get the pool)
    // For now, create a new pool directly

    let mut cfg = Config::new();
    cfg.url = Some(test_db_url());
    cfg.manager = Some(ManagerConfig {
        recycling_method: RecyclingMethod::Fast,
    });
    cfg.pool = Some(deadpool_postgres::PoolConfig::new(10));

    let pool = cfg.create_pool(Some(Runtime::Tokio1), NoTls).expect("Failed to create pool");

    PostgresIntrospector::new(pool)
}

#[tokio::test]
async fn test_get_columns_tf_sales() {
    let introspector = create_test_introspector().await;

    let columns = introspector.get_columns("tf_sales").await.expect("Failed to get columns");

    // Should have: id, revenue, quantity, cost, discount, data, customer_id, product_id,
    // occurred_at, created_at
    assert!(columns.len() >= 10, "Expected at least 10 columns, got {}", columns.len());

    // Check for key columns
    let column_names: Vec<String> = columns.iter().map(|(name, _, _)| name.clone()).collect();
    assert!(column_names.contains(&"revenue".to_string()));
    assert!(column_names.contains(&"quantity".to_string()));
    assert!(column_names.contains(&"data".to_string()));
    assert!(column_names.contains(&"customer_id".to_string()));
}

// ── Index definitions and view resolution (#1307) ────────────────────────
//
// These seed their own relations rather than reading a fixture another suite
// prepared: the property under test is about *which index a column sits in*,
// which a shared fixture could satisfy by accident and which a later edit to
// that fixture could silently remove.

/// Run DDL on the test database. Its own connection: the introspector holds a
/// read-only view of the catalog and its pool is private.
async fn exec(sql: &str) {
    let (client, connection) =
        tokio_postgres::connect(&test_db_url(), NoTls).await.expect("connect for DDL");
    tokio::spawn(async move {
        let _ = connection.await;
    });
    client.batch_execute(sql).await.expect("DDL");
}

#[tokio::test]
async fn index_definitions_distinguish_a_composite_from_separate_indexes() {
    let introspector = create_test_introspector().await;
    exec(
        "DROP TABLE IF EXISTS tb_idxdef_1307 CASCADE;
         CREATE TABLE tb_idxdef_1307 (pk integer PRIMARY KEY, status text, data jsonb);
         CREATE INDEX ix_1307_status ON tb_idxdef_1307 (status);
         CREATE INDEX ix_1307_pair ON tb_idxdef_1307 (status, pk);",
    )
    .await;

    let indexes = introspector
        .get_index_definitions("tb_idxdef_1307")
        .await
        .expect("index definitions");

    let single = indexes.iter().find(|i| i.name == "ix_1307_status").expect("single");
    let pair = indexes.iter().find(|i| i.name == "ix_1307_pair").expect("pair");

    // The distinction `get_indexed_columns` cannot make: it returns
    // {data?, pk, status} for this table either way.
    assert_eq!(single.keys, vec!["status".to_string()]);
    assert_eq!(pair.keys, vec!["status".to_string(), "pk".to_string()]);

    let pkey = indexes.iter().find(|i| i.unique).expect("the primary key is unique");
    assert_eq!(pkey.keys, vec!["pk".to_string()]);

    exec("DROP TABLE tb_idxdef_1307 CASCADE;").await;
}

/// An expression key has no `attnum`, so reading `indkey` alone would report
/// this index as having one key instead of two.
#[tokio::test]
async fn index_definitions_include_expression_keys() {
    let introspector = create_test_introspector().await;
    exec(
        "DROP TABLE IF EXISTS tb_idxexpr_1307 CASCADE;
         CREATE TABLE tb_idxexpr_1307 (pk integer PRIMARY KEY, status text, data jsonb);
         CREATE INDEX ix_1307_expr ON tb_idxexpr_1307 (status, ((data ->> 'id')));",
    )
    .await;

    let indexes = introspector
        .get_index_definitions("tb_idxexpr_1307")
        .await
        .expect("index definitions");
    let expr = indexes.iter().find(|i| i.name == "ix_1307_expr").expect("expression index");

    assert_eq!(expr.keys.len(), 2, "both keys are visible: {:?}", expr.keys);
    assert_eq!(expr.keys[0], "status");
    assert!(
        expr.keys[1].contains("->>"),
        "the expression key is rendered, not dropped: {:?}",
        expr.keys
    );

    exec("DROP TABLE tb_idxexpr_1307 CASCADE;").await;
}

#[tokio::test]
async fn a_view_resolves_to_the_table_that_carries_its_indexes() {
    let introspector = create_test_introspector().await;
    exec(
        "DROP VIEW IF EXISTS v_idxbase_1307;
         DROP TABLE IF EXISTS tb_idxbase_1307 CASCADE;
         CREATE TABLE tb_idxbase_1307 (pk integer PRIMARY KEY, status text);
         CREATE VIEW v_idxbase_1307 AS SELECT pk, status FROM tb_idxbase_1307;",
    )
    .await;

    // The premise: the view itself has none.
    assert!(
        introspector
            .get_index_definitions("v_idxbase_1307")
            .await
            .expect("view")
            .is_empty(),
        "a view carries no indexes, which is why resolution is needed"
    );
    assert_eq!(
        introspector.resolve_base_relations("v_idxbase_1307").await.expect("resolve"),
        vec!["tb_idxbase_1307".to_string()]
    );
    // A table is not a view and depends on nothing.
    assert!(
        introspector
            .resolve_base_relations("tb_idxbase_1307")
            .await
            .expect("table")
            .is_empty()
    );

    exec("DROP VIEW v_idxbase_1307; DROP TABLE tb_idxbase_1307 CASCADE;").await;
}

#[tokio::test]
async fn test_get_indexed_columns_tf_sales() {
    let introspector = create_test_introspector().await;

    let indexed = introspector
        .get_indexed_columns("tf_sales")
        .await
        .expect("Failed to get indexed columns");

    // Should have indexes on: id (PK), customer_id, product_id, occurred_at, data (GIN)
    assert!(indexed.len() >= 4, "Expected at least 4 indexed columns, got {}", indexed.len());

    assert!(indexed.contains(&"customer_id".to_string()));
    assert!(indexed.contains(&"product_id".to_string()));
    assert!(indexed.contains(&"occurred_at".to_string()));
}

#[tokio::test]
async fn test_database_type() {
    let introspector = create_test_introspector().await;
    assert_eq!(introspector.database_type(), DatabaseType::PostgreSQL);
}
