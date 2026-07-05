//! Tests for cron state migration DDL.

#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable
#![allow(clippy::print_stderr)] // Reason: skip message when no backing Postgres is available

use sqlx::PgPool;

use super::{cron_migration_sql, inbound_migration_sql, send_tracking_migration_sql};

// ── Helpers ───────────────────────────────────────────────────────────────────

/// Connect to the harness-provided Postgres (Dagger-bound in CI; a local spawn with
/// the `local-testcontainers` feature). Returns the pool plus the service guard, which
/// the caller holds so a locally-spawned container outlives the test. `None` when no
/// service is available (no `DATABASE_URL`, feature off) so the test skips cleanly.
async fn connect_pool() -> Option<(PgPool, fraiseql_test_support::Service)> {
    let svc = fraiseql_test_support::postgres().await?;
    let pool = PgPool::connect(svc.url()).await.unwrap();
    Some((pool, svc))
}

/// Execute multi-statement DDL by splitting on semicolons.
async fn execute_ddl(pool: &PgPool, ddl: &str) {
    for stmt in ddl.split(';') {
        let trimmed = stmt.trim();
        if !trimmed.is_empty() {
            sqlx::query(trimmed).execute(pool).await.unwrap();
        }
    }
}

/// Verify the cron DDL is syntactically complete and contains all expected
/// columns. Runs without a database — just checks the SQL string content.
#[test]
fn test_cron_migration_ddl_is_valid_sql() {
    let ddl = cron_migration_sql();

    assert!(
        ddl.contains("_fraiseql_cron_state"),
        "DDL must create _fraiseql_cron_state table"
    );

    // Must use IF NOT EXISTS for idempotency
    assert!(ddl.contains("IF NOT EXISTS"), "DDL must use IF NOT EXISTS");

    // Required columns
    for col in [
        "pk_cron_state",
        "function_name",
        "cron_expr",
        "last_fired_at",
        "next_fire_at",
        "fire_count",
        "updated_at",
    ] {
        assert!(ddl.contains(col), "DDL must contain column: {col}");
    }

    // Must have indexes
    assert!(ddl.contains("idx_cron_state_function"), "DDL must create function_name index");
    assert!(ddl.contains("idx_cron_state_next_fire"), "DDL must create next_fire_at index");

    // Trinity-style PK
    assert!(
        ddl.contains("GENERATED ALWAYS AS IDENTITY"),
        "pk must use GENERATED ALWAYS AS IDENTITY (Trinity pattern)"
    );

    // Unique constraint for idempotent scheduler state
    assert!(
        ddl.contains("UNIQUE (function_name, cron_expr)"),
        "DDL must have unique constraint on (function_name, cron_expr)"
    );
}

/// Verify the migration creates the `_fraiseql_cron_state` table in a real
/// PostgreSQL database.
#[tokio::test]
async fn test_cron_migration_creates_table() {
    let Some((pool, _svc)) = connect_pool().await else {
        eprintln!(
            "SKIP test_cron_migration_creates_table: no postgres (set DATABASE_URL or enable fraiseql-test-support/local-testcontainers)"
        );
        return;
    };

    let ddl = cron_migration_sql();
    execute_ddl(&pool, ddl).await;

    let (exists,): (bool,) = sqlx::query_as(
        "SELECT EXISTS (
            SELECT 1 FROM pg_class WHERE relname = '_fraiseql_cron_state'
        )",
    )
    .fetch_one(&pool)
    .await
    .unwrap();

    assert!(exists, "table _fraiseql_cron_state must exist after migration");
}

/// Verify the inbound-spine DDL is syntactically complete and contains all
/// expected columns and the dedup constraint. Runs without a database.
#[test]
fn test_inbound_migration_ddl_is_valid_sql() {
    let ddl = inbound_migration_sql();

    assert!(
        ddl.contains("_fraiseql_inbound_message"),
        "DDL must create _fraiseql_inbound_message table"
    );
    assert!(ddl.contains("IF NOT EXISTS"), "DDL must use IF NOT EXISTS");

    for col in [
        "pk_inbound_message",
        "source",
        "idempotency_key",
        "thread_key",
        "payload",
        "received_at",
        "created_at",
    ] {
        assert!(ddl.contains(col), "DDL must contain column: {col}");
    }

    // Dedup key is the at-least-once guarantee.
    assert!(
        ddl.contains("UNIQUE (source, idempotency_key)"),
        "DDL must dedup on (source, idempotency_key)"
    );
    assert!(
        ddl.contains("GENERATED ALWAYS AS IDENTITY"),
        "pk must use GENERATED ALWAYS AS IDENTITY (Trinity pattern)"
    );
    assert!(ddl.contains("idx_inbound_message_thread"), "DDL must create thread_key index");
    assert!(
        ddl.contains("idx_inbound_message_received"),
        "DDL must create received_at index"
    );
}

/// Verify the inbound migration creates the table in a real PostgreSQL database.
#[tokio::test]
async fn test_inbound_migration_creates_table() {
    let Some((pool, _svc)) = connect_pool().await else {
        eprintln!(
            "SKIP test_inbound_migration_creates_table: no postgres (set DATABASE_URL or enable fraiseql-test-support/local-testcontainers)"
        );
        return;
    };

    execute_ddl(&pool, inbound_migration_sql()).await;

    let (exists,): (bool,) = sqlx::query_as(
        "SELECT EXISTS (
            SELECT 1 FROM pg_class WHERE relname = '_fraiseql_inbound_message'
        )",
    )
    .fetch_one(&pool)
    .await
    .unwrap();

    assert!(exists, "table _fraiseql_inbound_message must exist after migration");
}

/// Verify the send-tracking DDL is syntactically complete and contains the
/// columns, exactly-once key, and tenant RLS. Runs without a database.
#[test]
fn test_send_tracking_migration_ddl_is_valid_sql() {
    let ddl = send_tracking_migration_sql();

    for table in ["_fraiseql_send_status", "_fraiseql_suppression"] {
        assert!(ddl.contains(table), "DDL must create {table}");
    }
    assert!(ddl.contains("IF NOT EXISTS"), "DDL must use IF NOT EXISTS");

    for col in [
        "send_id",
        "tenant_id",
        "recipient",
        "sending_address",
        "status",
        "challenge_count",
        "last_signal",
        "address_hash",
        "reason",
    ] {
        assert!(ddl.contains(col), "DDL must contain column: {col}");
    }

    // Exactly-once + suppression keys coalesce NULL tenants so single-tenant rows
    // are not treated as always-distinct.
    assert!(
        ddl.contains("COALESCE(tenant_id, '')"),
        "unique keys must coalesce NULL tenant_id"
    );
    // Tenant-scoped RLS for app-facing reads.
    assert!(ddl.contains("ENABLE ROW LEVEL SECURITY"), "DDL must enable RLS");
    assert!(
        ddl.contains("current_setting('fraiseql.tenant_id', true)"),
        "RLS policy must key on the fraiseql.tenant_id GUC"
    );
    // Policies are dropped-if-exists first so re-running the DDL is idempotent
    // (CREATE POLICY has no IF NOT EXISTS form).
    assert!(ddl.contains("DROP POLICY IF EXISTS"), "policies must be idempotent");
}

/// Verify the send-tracking migration creates both tables in a real PostgreSQL
/// database and is idempotent (re-running does not error).
#[tokio::test]
async fn test_send_tracking_migration_creates_tables_idempotently() {
    let Some((pool, _svc)) = connect_pool().await else {
        eprintln!(
            "SKIP test_send_tracking_migration_creates_tables_idempotently: no postgres (set DATABASE_URL or enable fraiseql-test-support/local-testcontainers)"
        );
        return;
    };

    let ddl = send_tracking_migration_sql();
    // Run twice — the RLS policy drop-and-recreate must keep it idempotent.
    execute_ddl(&pool, ddl).await;
    execute_ddl(&pool, ddl).await;

    for table in ["_fraiseql_send_status", "_fraiseql_suppression"] {
        let (exists,): (bool,) =
            sqlx::query_as("SELECT EXISTS (SELECT 1 FROM pg_class WHERE relname = $1)")
                .bind(table)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert!(exists, "table {table} must exist after migration");
    }
}

/// Verify the migration is idempotent — running it twice does not error.
#[tokio::test]
async fn test_cron_migration_is_idempotent() {
    let Some((pool, _svc)) = connect_pool().await else {
        eprintln!(
            "SKIP test_cron_migration_is_idempotent: no postgres (set DATABASE_URL or enable fraiseql-test-support/local-testcontainers)"
        );
        return;
    };

    let ddl = cron_migration_sql();

    // Run twice — second run must not error
    execute_ddl(&pool, ddl).await;
    execute_ddl(&pool, ddl).await;
}
