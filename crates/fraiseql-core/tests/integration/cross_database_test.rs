//! Cross-Database Parity Tests
//!
//! Validates that equivalent queries produce semantically equivalent results
//! across PostgreSQL and MySQL adapters using real database containers.
//!
//! All tests are `#[ignore]` and require Docker. Run with:
//! ```bash
//! cargo nextest run --test cross_database_test --run-ignored \
//!     --features test-mysql,test-postgres
//! ```
//!
//! The test schema uses a minimal `v_cross_item` view that returns a `data` JSON/JSONB
//! column, matching the fraiseql adapter contract.

#![cfg(all(feature = "test-postgres", feature = "test-mysql"))]
#![allow(clippy::unwrap_used)]  // Reason: test setup code, panics are acceptable

use fraiseql_core::db::{
    WhereClause, WhereOperator,
    mysql::MySqlAdapter,
    postgres::PostgresAdapter,
    traits::DatabaseAdapter,
};
use serde_json::json;
use testcontainers_modules::{
    mysql::Mysql,
    postgres::Postgres,
    testcontainers::runners::AsyncRunner,
};

// ─────────────────────────────────────────────────────────────────────────────
// Shared schema SQL (per database)
// ─────────────────────────────────────────────────────────────────────────────

const PG_SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS tb_cross_item (
    id   CHAR(36) NOT NULL PRIMARY KEY,
    name TEXT     NOT NULL UNIQUE,
    age  INT,
    data JSONB    NOT NULL
);
CREATE OR REPLACE VIEW v_cross_item AS
    SELECT data FROM tb_cross_item;
"#;

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

const MYSQL_SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS tb_cross_item (
    id   CHAR(36)     NOT NULL PRIMARY KEY,
    name VARCHAR(255) NOT NULL,
    age  INT,
    data JSON         NOT NULL,
    UNIQUE KEY uk_cross_item_name (name)
);
CREATE OR REPLACE VIEW v_cross_item AS
    SELECT data FROM tb_cross_item;
"#;

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

async fn setup_postgres() -> (PostgresAdapter, impl Drop) {
    let container = Postgres::default()
        .start()
        .await
        .expect("Failed to start PostgreSQL container");

    let port = container
        .get_host_port_ipv4(5432)
        .await
        .expect("Failed to get container port");

    let conn_str = format!("host=127.0.0.1 port={port} user=postgres password=postgres dbname=postgres");

    // Apply schema and seed via tokio_postgres
    let (client, conn) = tokio_postgres::connect(&conn_str, tokio_postgres::NoTls)
        .await
        .expect("Failed to connect to PG container for setup");
    tokio::spawn(async move {
        if let Err(e) = conn.await {
            eprintln!("PG connection error during setup: {e}");
        }
    });
    client.batch_execute(PG_SCHEMA).await.expect("Failed to apply PG schema");
    client.batch_execute(PG_SEED).await.expect("Failed to seed PG data");

    let adapter_str = format!(
        "postgres://postgres:postgres@127.0.0.1:{port}/postgres"
    );
    let adapter = PostgresAdapter::new(&adapter_str)
        .await
        .expect("Failed to create PostgresAdapter");

    (adapter, container)
}

async fn setup_mysql() -> (MySqlAdapter, impl Drop) {
    let container = Mysql::default()
        .start()
        .await
        .expect("Failed to start MySQL container");

    let port = container
        .get_host_port_ipv4(3306)
        .await
        .expect("Failed to get container port");

    // MySQL default image: user=root, no password, db=test
    let conn_str = format!("mysql://root@127.0.0.1:{port}/test");

    // Apply schema and seed via sqlx
    let pool = sqlx::MySqlPool::connect(&conn_str)
        .await
        .expect("Failed to connect to MySQL container");

    sqlx::query(MYSQL_SCHEMA)
        .execute(&pool)
        .await
        .expect("Failed to apply MySQL schema");
    sqlx::query(MYSQL_SEED)
        .execute(&pool)
        .await
        .expect("Failed to seed MySQL data");
    drop(pool);

    let adapter = MySqlAdapter::new(&conn_str)
        .await
        .expect("Failed to create MySqlAdapter");

    (adapter, container)
}

// ─────────────────────────────────────────────────────────────────────────────
// Test 1: Same query returns the same field set
// ─────────────────────────────────────────────────────────────────────────────

#[tokio::test]
#[ignore = "requires docker — cargo nextest run --test cross_database_test --run-ignored --features test-mysql,test-postgres"]
async fn same_query_returns_same_fields_on_pg_and_mysql() {
    let (pg, _pg_c) = setup_postgres().await;
    let (my, _my_c) = setup_mysql().await;

    let pg_rows = pg
        .execute_where_query("v_cross_item", None, Some(10), None)
        .await
        .expect("PG query failed");

    let my_rows = my
        .execute_where_query("v_cross_item", None, Some(10), None)
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
#[ignore = "requires docker — cargo nextest run --test cross_database_test --run-ignored --features test-mysql,test-postgres"]
async fn where_eq_operator_returns_same_results_on_pg_and_mysql() {
    let (pg, _pg_c) = setup_postgres().await;
    let (my, _my_c) = setup_mysql().await;

    let clause = WhereClause::Field {
        path:     vec!["name".to_string()],
        operator: WhereOperator::Eq,
        value:    json!("alice"),
    };

    let pg_rows = pg
        .execute_where_query("v_cross_item", Some(&clause), None, None)
        .await
        .expect("PG query failed");

    let my_rows = my
        .execute_where_query("v_cross_item", Some(&clause), None, None)
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
#[ignore = "requires docker — cargo nextest run --test cross_database_test --run-ignored --features test-mysql,test-postgres"]
async fn where_gte_operator_returns_same_count_on_pg_and_mysql() {
    let (pg, _pg_c) = setup_postgres().await;
    let (my, _my_c) = setup_mysql().await;

    let clause = WhereClause::Field {
        path:     vec!["age".to_string()],
        operator: WhereOperator::Gte,
        value:    json!(30),
    };

    let pg_rows = pg
        .execute_where_query("v_cross_item", Some(&clause), None, None)
        .await
        .expect("PG query failed");

    let my_rows = my
        .execute_where_query("v_cross_item", Some(&clause), None, None)
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
#[ignore = "requires docker — cargo nextest run --test cross_database_test --run-ignored --features test-mysql,test-postgres"]
async fn null_fields_represented_identically_across_adapters() {
    let (pg, _pg_c) = setup_postgres().await;
    let (my, _my_c) = setup_mysql().await;

    // alice has "score": null
    let clause = WhereClause::Field {
        path:     vec!["name".to_string()],
        operator: WhereOperator::Eq,
        value:    json!("alice"),
    };

    let pg_rows = pg
        .execute_where_query("v_cross_item", Some(&clause), None, None)
        .await
        .expect("PG query failed");

    let my_rows = my
        .execute_where_query("v_cross_item", Some(&clause), None, None)
        .await
        .expect("MySQL query failed");

    assert_eq!(pg_rows.len(), 1);
    assert_eq!(my_rows.len(), 1);

    let pg_score = &pg_rows[0].as_value()["score"];
    let my_score = &my_rows[0].as_value()["score"];

    assert!(
        pg_score.is_null(),
        "PG score should be null for alice, got: {pg_score}"
    );
    assert!(
        my_score.is_null(),
        "MySQL score should be null for alice, got: {my_score}"
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// Test 5: LIMIT is respected consistently
// ─────────────────────────────────────────────────────────────────────────────

#[tokio::test]
#[ignore = "requires docker — cargo nextest run --test cross_database_test --run-ignored --features test-mysql,test-postgres"]
async fn limit_is_respected_consistently_across_adapters() {
    let (pg, _pg_c) = setup_postgres().await;
    let (my, _my_c) = setup_mysql().await;

    let pg_rows = pg
        .execute_where_query("v_cross_item", None, Some(2), None)
        .await
        .expect("PG query failed");

    let my_rows = my
        .execute_where_query("v_cross_item", None, Some(2), None)
        .await
        .expect("MySQL query failed");

    assert_eq!(pg_rows.len(), 2, "PG should honour LIMIT 2");
    assert_eq!(my_rows.len(), 2, "MySQL should honour LIMIT 2");
}
