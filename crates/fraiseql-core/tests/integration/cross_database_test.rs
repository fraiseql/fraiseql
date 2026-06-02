//! Cross-Database Parity Tests
//!
//! Validates that equivalent queries produce semantically equivalent results
//! across the PostgreSQL and MySQL adapters using the harness-provided services.
//!
//! They skip automatically unless `FEDERATION_TESTS` is set, and read DATABASE_URL /
//! MYSQL_URL via the harness:
//! ```bash
//! dagger call test-integration --suite=cross-db
//! ```
//!
//! The test schema uses a minimal `v_cross_item` view that returns a `data` JSON/JSONB
//! column, matching the fraiseql adapter contract.

#![cfg(all(feature = "test-postgres", feature = "test-mysql"))]
#![allow(clippy::unwrap_used, clippy::print_stdout, clippy::print_stderr)] // Reason: test setup code, panics are acceptable

use fraiseql_core::db::{
    WhereClause, WhereOperator, mysql::MySqlAdapter, postgres::PostgresAdapter,
    traits::DatabaseAdapter,
};
use serde_json::json;

// ─────────────────────────────────────────────────────────────────────────────
// Shared schema SQL (per database)
// ─────────────────────────────────────────────────────────────────────────────

const PG_SCHEMA: &str = r"
CREATE TABLE IF NOT EXISTS tb_cross_item (
    id   CHAR(36) NOT NULL PRIMARY KEY,
    name TEXT     NOT NULL UNIQUE,
    age  INT,
    data JSONB    NOT NULL
);
CREATE OR REPLACE VIEW v_cross_item AS
    SELECT data FROM tb_cross_item;
";

const PG_SEED: &str = r#"
INSERT INTO tb_cross_item (id, name, age, data) VALUES
    ('aaaaaaaa-0000-0000-0000-000000000001', 'alice', 30,
     '{"name": "alice", "age": 30, "active": true,  "score": null}'),
    ('aaaaaaaa-0000-0000-0000-000000000002', 'bob',   25,
     '{"name": "bob",   "age": 25, "active": false, "score": 42}'),
    ('aaaaaaaa-0000-0000-0000-000000000003', 'carol', 35,
     '{"name": "carol", "age": 35, "active": true,  "score": 100}')
ON CONFLICT DO NOTHING;
"#;

const MYSQL_SCHEMA: &str = r"
CREATE TABLE IF NOT EXISTS tb_cross_item (
    id   CHAR(36)     NOT NULL PRIMARY KEY,
    name VARCHAR(255) NOT NULL,
    age  INT,
    data JSON         NOT NULL,
    UNIQUE KEY uk_cross_item_name (name)
);
CREATE OR REPLACE VIEW v_cross_item AS
    SELECT data FROM tb_cross_item;
";

const MYSQL_SEED: &str = r#"
INSERT IGNORE INTO tb_cross_item (id, name, age, data) VALUES
    ('aaaaaaaa-0000-0000-0000-000000000001', 'alice', 30,
     '{"name": "alice", "age": 30, "active": true,  "score": null}'),
    ('aaaaaaaa-0000-0000-0000-000000000002', 'bob',   25,
     '{"name": "bob",   "age": 25, "active": false, "score": 42}'),
    ('aaaaaaaa-0000-0000-0000-000000000003', 'carol', 35,
     '{"name": "carol", "age": 35, "active": true,  "score": 100}');
"#;

// ─────────────────────────────────────────────────────────────────────────────
// Container helpers
// ─────────────────────────────────────────────────────────────────────────────

async fn setup_postgres() -> (PostgresAdapter, fraiseql_test_support::Service) {
    let svc = fraiseql_test_support::postgres()
        .await
        .expect("DATABASE_URL must be set for cross-db tests (dagger call test-integration --suite=cross-db)");
    let url = svc.url().to_string();

    // Apply schema and seed via tokio_postgres
    let (client, conn) = tokio_postgres::connect(&url, tokio_postgres::NoTls)
        .await
        .expect("Failed to connect to PG for setup");
    tokio::spawn(async move {
        if let Err(e) = conn.await {
            eprintln!("PG connection error during setup: {e}");
        }
    });
    client.batch_execute(PG_SCHEMA).await.expect("Failed to apply PG schema");
    client.batch_execute(PG_SEED).await.expect("Failed to seed PG data");

    let adapter = PostgresAdapter::new(&url).await.expect("Failed to create PostgresAdapter");

    (adapter, svc)
}

async fn setup_mysql() -> (MySqlAdapter, fraiseql_test_support::Service) {
    let svc = fraiseql_test_support::mysql().await.expect(
        "MYSQL_URL must be set for cross-db tests (dagger call test-integration --suite=cross-db)",
    );
    let url = svc.url().to_string();

    // Apply schema and seed via sqlx. `sqlx::query` runs a single statement, so the
    // multi-statement schema is split on `;` (each statement is idempotent: CREATE …
    // IF NOT EXISTS / CREATE OR REPLACE VIEW, and the seed uses INSERT IGNORE).
    let pool = sqlx::MySqlPool::connect(&url).await.expect("Failed to connect to MySQL");

    for stmt in MYSQL_SCHEMA.split(';') {
        let stmt = stmt.trim();
        if !stmt.is_empty() {
            sqlx::query(stmt)
                .execute(&pool)
                .await
                .expect("Failed to apply MySQL schema statement");
        }
    }
    sqlx::query(MYSQL_SEED.trim().trim_end_matches(';'))
        .execute(&pool)
        .await
        .expect("Failed to seed MySQL data");
    drop(pool);

    let adapter = MySqlAdapter::new(&url).await.expect("Failed to create MySqlAdapter");

    (adapter, svc)
}

// ─────────────────────────────────────────────────────────────────────────────
// Test 1: Same query returns the same field set
// ─────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn same_query_returns_same_fields_on_pg_and_mysql() {
    if std::env::var("FEDERATION_TESTS").is_err() {
        eprintln!("Skipping: FEDERATION_TESTS not set");
        return;
    }
    let (pg, _pg_c) = setup_postgres().await;
    let (my, _my_c) = setup_mysql().await;

    let pg_rows = pg
        .execute_where_query("v_cross_item", None, Some(10), None, None)
        .await
        .expect("PG query failed");

    let my_rows = my
        .execute_where_query("v_cross_item", None, Some(10), None, None)
        .await
        .expect("MySQL query failed");

    assert_eq!(pg_rows.len(), 3, "PG should return 3 rows");
    assert_eq!(my_rows.len(), 3, "MySQL should return 3 rows");

    let pg_keys: std::collections::BTreeSet<_> = pg_rows[0]
        .as_value()
        .as_object()
        .expect("PG row should be an object")
        .keys()
        .cloned()
        .collect();

    let my_keys: std::collections::BTreeSet<_> = my_rows[0]
        .as_value()
        .as_object()
        .expect("MySQL row should be an object")
        .keys()
        .cloned()
        .collect();

    assert_eq!(pg_keys, my_keys, "Field set must match across adapters");
}

// ─────────────────────────────────────────────────────────────────────────────
// Test 2: WHERE eq operator returns equivalent results
// ─────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn where_eq_operator_returns_same_results_on_pg_and_mysql() {
    if std::env::var("FEDERATION_TESTS").is_err() {
        eprintln!("Skipping: FEDERATION_TESTS not set");
        return;
    }
    let (pg, _pg_c) = setup_postgres().await;
    let (my, _my_c) = setup_mysql().await;

    let clause = WhereClause::Field {
        path:     vec!["name".to_string()],
        operator: WhereOperator::Eq,
        value:    json!("alice"),
    };

    let pg_rows = pg
        .execute_where_query("v_cross_item", Some(&clause), None, None, None)
        .await
        .expect("PG query failed");

    let my_rows = my
        .execute_where_query("v_cross_item", Some(&clause), None, None, None)
        .await
        .expect("MySQL query failed");

    assert_eq!(pg_rows.len(), 1, "PG should return 1 row for name=alice");
    assert_eq!(my_rows.len(), 1, "MySQL should return 1 row for name=alice");

    let pg_name = pg_rows[0].as_value()["name"].as_str().expect("PG name should be a string");
    let my_name = my_rows[0].as_value()["name"].as_str().expect("MySQL name should be a string");

    assert_eq!(pg_name, "alice");
    assert_eq!(my_name, "alice");
    assert_eq!(pg_name, my_name, "name field must be identical across adapters");
}

// ─────────────────────────────────────────────────────────────────────────────
// Test 3: WHERE with numeric comparison (gte)
// ─────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn where_gte_operator_returns_same_count_on_pg_and_mysql() {
    if std::env::var("FEDERATION_TESTS").is_err() {
        eprintln!("Skipping: FEDERATION_TESTS not set");
        return;
    }
    let (pg, _pg_c) = setup_postgres().await;
    let (my, _my_c) = setup_mysql().await;

    let clause = WhereClause::Field {
        path:     vec!["age".to_string()],
        operator: WhereOperator::Gte,
        value:    json!(30),
    };

    let pg_rows = pg
        .execute_where_query("v_cross_item", Some(&clause), None, None, None)
        .await
        .expect("PG query failed");

    let my_rows = my
        .execute_where_query("v_cross_item", Some(&clause), None, None, None)
        .await
        .expect("MySQL query failed");

    assert_eq!(
        pg_rows.len(),
        my_rows.len(),
        "Both adapters should return the same number of rows for age >= 30"
    );
    assert_eq!(pg_rows.len(), 2, "alice (30) and carol (35) match age >= 30");
}

// ─────────────────────────────────────────────────────────────────────────────
// Test 4: NULL fields represented identically
// ─────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn null_fields_represented_identically_across_adapters() {
    if std::env::var("FEDERATION_TESTS").is_err() {
        eprintln!("Skipping: FEDERATION_TESTS not set");
        return;
    }
    let (pg, _pg_c) = setup_postgres().await;
    let (my, _my_c) = setup_mysql().await;

    // alice has "score": null
    let clause = WhereClause::Field {
        path:     vec!["name".to_string()],
        operator: WhereOperator::Eq,
        value:    json!("alice"),
    };

    let pg_rows = pg
        .execute_where_query("v_cross_item", Some(&clause), None, None, None)
        .await
        .expect("PG query failed");

    let my_rows = my
        .execute_where_query("v_cross_item", Some(&clause), None, None, None)
        .await
        .expect("MySQL query failed");

    assert_eq!(pg_rows.len(), 1);
    assert_eq!(my_rows.len(), 1);

    let pg_score = &pg_rows[0].as_value()["score"];
    let my_score = &my_rows[0].as_value()["score"];

    assert!(pg_score.is_null(), "PG score should be null for alice, got: {pg_score}");
    assert!(my_score.is_null(), "MySQL score should be null for alice, got: {my_score}");
}

// ─────────────────────────────────────────────────────────────────────────────
// Test 5: LIMIT is respected consistently
// ─────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn limit_is_respected_consistently_across_adapters() {
    if std::env::var("FEDERATION_TESTS").is_err() {
        eprintln!("Skipping: FEDERATION_TESTS not set");
        return;
    }
    let (pg, _pg_c) = setup_postgres().await;
    let (my, _my_c) = setup_mysql().await;

    let pg_rows = pg
        .execute_where_query("v_cross_item", None, Some(2), None, None)
        .await
        .expect("PG query failed");

    let my_rows = my
        .execute_where_query("v_cross_item", None, Some(2), None, None)
        .await
        .expect("MySQL query failed");

    assert_eq!(pg_rows.len(), 2, "PG should honour LIMIT 2");
    assert_eq!(my_rows.len(), 2, "MySQL should honour LIMIT 2");
}
