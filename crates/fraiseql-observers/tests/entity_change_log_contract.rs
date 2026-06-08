//! Change-log contract migration tests (Change Spine, phase-00).
//!
//! Verifies that `migrations/08_create_entity_change_log_contract.sql` (exposed
//! as [`fraiseql_observers::migrations::entity_change_log_contract_sql`]) installs
//! the owned `core.tb_entity_change_log` superset table + indexes + the
//! `core.v_entity_change_log` read-path view, and reconciles a pre-existing
//! app-created table **additively** (no rename, no data loss).
//!
//! ## Running
//!
//! These tests share one `core.tb_entity_change_log`, so run them serially:
//!
//! ```bash
//! DATABASE_URL=postgresql://fraiseql_test:fraiseql_test_password@localhost:5433/test_fraiseql \
//!   cargo test -p fraiseql-observers --test entity_change_log_contract \
//!   -- --ignored --test-threads=1
//! ```

#![cfg(feature = "postgres")]
#![allow(clippy::unwrap_used)] // Reason: integration test file

use fraiseql_observers::migrations::{
    ENTITY_CHANGE_LOG_CONTRACT_COLUMNS, entity_change_log_contract_sql,
};
use fraiseql_test_utils::database_url;
use sqlx::{PgPool, Row, postgres::PgPoolOptions};

async fn pool() -> PgPool {
    PgPoolOptions::new()
        .max_connections(2)
        .connect(&database_url())
        .await
        .expect("connect to test database")
}

/// Wipe the contract objects so each test starts from a known slate.
async fn drop_contract(pool: &PgPool) {
    sqlx::query("CREATE SCHEMA IF NOT EXISTS core").execute(pool).await.unwrap();
    sqlx::query("DROP VIEW IF EXISTS core.v_entity_change_log")
        .execute(pool)
        .await
        .unwrap();
    sqlx::query("DROP TABLE IF EXISTS core.tb_entity_change_log CASCADE")
        .execute(pool)
        .await
        .unwrap();
}

/// Column names present on `core.tb_entity_change_log`.
async fn live_columns(pool: &PgPool) -> Vec<String> {
    sqlx::query(
        "SELECT column_name FROM information_schema.columns
         WHERE table_schema = 'core' AND table_name = 'tb_entity_change_log'",
    )
    .fetch_all(pool)
    .await
    .unwrap()
    .into_iter()
    .map(|r| r.get::<String, _>("column_name"))
    .collect()
}

#[tokio::test]
#[ignore = "requires PostgreSQL — run with --ignored --test-threads=1"]
async fn migration_creates_table_with_all_contract_columns() {
    let pool = pool().await;
    drop_contract(&pool).await;

    sqlx::raw_sql(entity_change_log_contract_sql()).execute(&pool).await.unwrap();

    let cols = live_columns(&pool).await;
    for expected in ENTITY_CHANGE_LOG_CONTRACT_COLUMNS {
        assert!(
            cols.iter().any(|c| c == expected),
            "fresh contract table is missing column `{expected}` (have: {cols:?})"
        );
    }
}

#[tokio::test]
#[ignore = "requires PostgreSQL — run with --ignored --test-threads=1"]
async fn migration_reconciles_existing_app_table() {
    let pool = pool().await;
    drop_contract(&pool).await;

    // A minimal pre-existing app-created table with one seeded row.
    sqlx::query(
        r"
        CREATE TABLE core.tb_entity_change_log (
            pk_entity_change_log BIGSERIAL PRIMARY KEY,
            fk_customer_org      BIGINT,
            object_type          TEXT NOT NULL,
            modification_type    TEXT NOT NULL,
            created_at           TIMESTAMPTZ NOT NULL DEFAULT now()
        )
        ",
    )
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO core.tb_entity_change_log (fk_customer_org, object_type, modification_type)
         VALUES (42, 'User', 'INSERT')",
    )
    .execute(&pool)
    .await
    .unwrap();

    // Reconcile to the contract — and a second run to prove idempotency.
    sqlx::raw_sql(entity_change_log_contract_sql()).execute(&pool).await.unwrap();
    sqlx::raw_sql(entity_change_log_contract_sql()).execute(&pool).await.unwrap();

    // (a) The seeded row survives, with fk_customer_org intact (NOT renamed).
    let row = sqlx::query(
        "SELECT fk_customer_org, object_type FROM core.tb_entity_change_log
         WHERE object_type = 'User'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    let org: i64 = row.get("fk_customer_org");
    assert_eq!(org, 42, "fk_customer_org value preserved through the additive migration");

    // (b) The new contract columns were added.
    let cols = live_columns(&pool).await;
    for expected in [
        "tenant_id",
        "id",
        "duration_ms",
        "started_at",
        "seq",
        "actor_type",
        "acting_for",
        "schema_version",
        "trace_id",
        "trace_context",
        "commit_time",
        "updated_fields",
        "cascade",
    ] {
        assert!(
            cols.iter().any(|c| c == expected),
            "reconcile did not add contract column `{expected}` (have: {cols:?})"
        );
    }
    // fk_customer_org is still present alongside the new tenant_id.
    assert!(cols.iter().any(|c| c == "fk_customer_org"), "fk_customer_org kept");
    assert!(cols.iter().any(|c| c == "tenant_id"), "tenant_id added");

    // (c) The defaulted backbone backfilled on the existing row.
    let backfilled: i64 =
        sqlx::query("SELECT count(*) AS n FROM core.tb_entity_change_log WHERE id IS NOT NULL")
            .fetch_one(&pool)
            .await
            .unwrap()
            .get("n");
    assert_eq!(backfilled, 1, "id default backfilled the pre-existing row");
}

#[tokio::test]
#[ignore = "requires PostgreSQL — run with --ignored --test-threads=1"]
async fn migration_creates_duration_index() {
    let pool = pool().await;
    drop_contract(&pool).await;
    sqlx::raw_sql(entity_change_log_contract_sql()).execute(&pool).await.unwrap();

    let def: String = sqlx::query(
        "SELECT indexdef FROM pg_indexes
         WHERE schemaname = 'core' AND tablename = 'tb_entity_change_log'
           AND indexname = 'idx_entity_log_duration'",
    )
    .fetch_one(&pool)
    .await
    .unwrap()
    .get("indexdef");
    assert!(
        def.contains("duration_ms DESC"),
        "idx_entity_log_duration must order duration_ms DESC, got: {def}"
    );
}

#[tokio::test]
#[ignore = "requires PostgreSQL — run with --ignored --test-threads=1"]
async fn migration_creates_seq_indexes() {
    let pool = pool().await;
    drop_contract(&pool).await;
    sqlx::raw_sql(entity_change_log_contract_sql()).execute(&pool).await.unwrap();

    let names: Vec<String> = sqlx::query(
        "SELECT indexname FROM pg_indexes
         WHERE schemaname = 'core' AND tablename = 'tb_entity_change_log'",
    )
    .fetch_all(&pool)
    .await
    .unwrap()
    .into_iter()
    .map(|r| r.get::<String, _>("indexname"))
    .collect();
    assert!(names.iter().any(|n| n == "idx_entity_log_tenant_seq"), "(tenant_id, seq) index");
    assert!(names.iter().any(|n| n == "idx_entity_log_type_seq"), "(object_type, seq) index");
}

#[tokio::test]
#[ignore = "requires PostgreSQL — run with --ignored --test-threads=1"]
async fn view_exposes_duration_ms() {
    let pool = pool().await;
    drop_contract(&pool).await;
    sqlx::raw_sql(entity_change_log_contract_sql()).execute(&pool).await.unwrap();

    // The perf/envelope columns are queryable top-level on the view, and the
    // #149 GraphQL `data` JSONB is still projected.
    sqlx::query(
        "SELECT duration_ms, started_at, object_type, modification_type, object_id,
                tenant_id, created_at, trace_id, seq,
                data->>'object_type' AS gql_object_type
         FROM core.v_entity_change_log",
    )
    .fetch_all(&pool)
    .await
    .expect("v_entity_change_log exposes duration_ms + perf columns and the #149 data JSONB");
}
