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

use fraiseql_observers::{
    listener::{ChangeLogListener, ChangeLogListenerConfig},
    migrations::{ENTITY_CHANGE_LOG_CONTRACT_COLUMNS, entity_change_log_contract_sql},
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
async fn poller_decodes_executor_written_contract_rows() {
    let pool = pool().await;
    drop_contract(&pool).await;
    sqlx::raw_sql(entity_change_log_contract_sql()).execute(&pool).await.unwrap();

    // An executor-written contract row: object_id is a UUID, fk_customer_org /
    // fk_contact are BIGINT, plus the additive perf/envelope columns. The poller
    // previously decoded object_id/fk_customer_org as String and would fail the
    // sqlx type-check here; the Trinity-typed decode reconciles it. object_data
    // may legitimately be NULL on the contract — proved by omitting it. seq is
    // omitted so the SEQUENCE default assigns it.
    let tenant = uuid::Uuid::new_v4();
    sqlx::query(
        "INSERT INTO core.tb_entity_change_log
           (object_type, modification_type, object_id, fk_customer_org, fk_contact,
            tenant_id, duration_ms, commit_time)
         VALUES ('User', 'INSERT', gen_random_uuid(), 42, 7, $1, 5, now())",
    )
    .bind(tenant)
    .execute(&pool)
    .await
    .unwrap();

    let mut listener = ChangeLogListener::new(ChangeLogListenerConfig::new(pool.clone()));
    let batch = listener.next_batch().await.expect("poller decodes the contract row");

    assert_eq!(batch.len(), 1, "poller surfaced the executor-written row");
    let entry = &batch[0];
    assert_eq!(entry.object_type, "User");
    assert_eq!(entry.modification_type, "INSERT");
    assert_eq!(
        entry.fk_customer_org, "42",
        "BIGINT fk_customer_org surfaced as its decimal string"
    );
    assert_eq!(entry.fk_contact.as_deref(), Some("7"), "BIGINT fk_contact surfaced as a string");
    assert!(
        uuid::Uuid::parse_str(&entry.object_id).is_ok(),
        "object_id decoded from the UUID column: {}",
        entry.object_id
    );
    assert!(
        entry.object_data.is_null(),
        "NULL object_data decodes to JSON null, not an error"
    );

    // The Change-Spine envelope/perf columns are now projected top-level. Trinity:
    // tenant_id is the public-facing UUID partition stamp, NOT fk_customer_org.
    assert_eq!(
        entry.tenant_id.as_deref(),
        Some(tenant.to_string().as_str()),
        "tenant_id surfaced as the public-facing UUID"
    );
    assert_ne!(
        entry.tenant_id.as_deref(),
        Some("42"),
        "tenant_id must not be the fk_customer_org BIGINT"
    );
    assert_eq!(entry.duration_ms, Some(5), "duration_ms projected top-level");
    assert!(
        entry.seq.is_some_and(|s| s > 0),
        "seq projected from the SEQUENCE default: {:?}",
        entry.seq
    );
}

/// The `data_type` of a `core.tb_entity_change_log` column from the catalog.
async fn column_type(pool: &PgPool, column: &str) -> String {
    sqlx::query(
        "SELECT data_type FROM information_schema.columns
         WHERE table_schema = 'core' AND table_name = 'tb_entity_change_log'
           AND column_name = $1",
    )
    .bind(column)
    .fetch_one(pool)
    .await
    .unwrap()
    .get::<String, _>("data_type")
}

#[tokio::test]
#[ignore = "requires PostgreSQL — run with --ignored --test-threads=1"]
async fn tenant_id_is_uuid_and_fk_customer_org_is_bigint() {
    let pool = pool().await;
    drop_contract(&pool).await;
    sqlx::raw_sql(entity_change_log_contract_sql()).execute(&pool).await.unwrap();

    // Trinity: tenant_id is the public-facing UUID (the RLS/JWT partition stamp);
    // fk_customer_org is the internal BIGINT join FK. Complementary, not a rename.
    assert_eq!(column_type(&pool, "tenant_id").await, "uuid", "tenant_id is UUID");
    assert_eq!(
        column_type(&pool, "fk_customer_org").await,
        "bigint",
        "fk_customer_org is BIGINT"
    );
}

#[tokio::test]
#[ignore = "requires PostgreSQL — run with --ignored --test-threads=1"]
async fn seq_default_assigns_monotonic_values_to_any_insert() {
    let pool = pool().await;
    drop_contract(&pool).await;
    sqlx::raw_sql(entity_change_log_contract_sql()).execute(&pool).await.unwrap();

    // A cooperative external producer INSERT that omits seq still gets one from
    // the SEQUENCE default — the contract guarantees seq for ANY INSERTer, not
    // just the FraiseQL executor (the dedup basis is (object_type, seq)).
    for object_type in ["A", "B", "C"] {
        sqlx::query(
            "INSERT INTO core.tb_entity_change_log (object_type, modification_type)
             VALUES ($1, 'INSERT')",
        )
        .bind(object_type)
        .execute(&pool)
        .await
        .unwrap();
    }

    let seqs: Vec<i64> =
        sqlx::query("SELECT seq FROM core.tb_entity_change_log ORDER BY pk_entity_change_log")
            .fetch_all(&pool)
            .await
            .unwrap()
            .into_iter()
            .map(|r| r.get::<i64, _>("seq"))
            .collect();
    assert_eq!(seqs.len(), 3, "every insert got a row");
    assert!(seqs.iter().all(|&s| s > 0), "seq populated by the default: {seqs:?}");
    assert!(seqs.windows(2).all(|w| w[1] > w[0]), "seq is monotonic: {seqs:?}");
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
