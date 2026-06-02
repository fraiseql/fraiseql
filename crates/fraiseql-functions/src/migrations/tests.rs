//! Tests for cron state migration DDL.

#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable
#![allow(clippy::print_stderr)] // Reason: skip message when no backing Postgres is available

use sqlx::PgPool;

use super::cron_migration_sql;

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
