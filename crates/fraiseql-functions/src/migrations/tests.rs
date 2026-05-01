//! Tests for cron state migration DDL.

#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

use sqlx::PgPool;
use testcontainers::runners::AsyncRunner;
use testcontainers_modules::postgres::Postgres;

use super::cron_migration_sql;

// ── Helpers ───────────────────────────────────────────────────────────────────

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
    assert!(
        ddl.contains("idx_cron_state_function"),
        "DDL must create function_name index"
    );
    assert!(
        ddl.contains("idx_cron_state_next_fire"),
        "DDL must create next_fire_at index"
    );

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
    let container = Postgres::default().start().await.unwrap();
    let port = container.get_host_port_ipv4(5432).await.unwrap();
    let url = format!("postgres://postgres:postgres@127.0.0.1:{port}/postgres");
    let pool = PgPool::connect(&url).await.unwrap();

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
    let container = Postgres::default().start().await.unwrap();
    let port = container.get_host_port_ipv4(5432).await.unwrap();
    let url = format!("postgres://postgres:postgres@127.0.0.1:{port}/postgres");
    let pool = PgPool::connect(&url).await.unwrap();

    let ddl = cron_migration_sql();

    // Run twice — second run must not error
    execute_ddl(&pool, ddl).await;
    execute_ddl(&pool, ddl).await;
}
