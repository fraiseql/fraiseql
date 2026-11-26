//! Tests for cron state migration DDL.

#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable
#![allow(clippy::print_stderr)] // Reason: skip message when no backing Postgres is available

use sqlx::PgPool;

use super::{
    cron_migration_sql, dlq_migration_sql, inbound_migration_sql, send_tracking_migration_sql,
};

#[path = "boot_locks_tests.rs"]
mod boot_locks;

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

/// Execute multi-statement DDL as the server does: one script, sent whole behind the
/// migration lock. Splitting it on semicolons would cut a `DO` block's body apart, and
/// without the lock two tests migrating at once can deadlock on each other's DDL.
async fn execute_ddl(pool: &PgPool, ddl: &str) {
    // Reason: the crate's own migration DDL, behind the fixed lock statement.
    sqlx::raw_sql(sqlx::AssertSqlSafe(fraiseql_guard::ddl::lock_prefixed(ddl)))
        .execute(pool)
        .await
        .unwrap();
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

    // Exactly-once + suppression keys treat a NULL tenant as one space
    // (NULLS NOT DISTINCT), so single-tenant rows are not always-distinct. The
    // COALESCE expression indexes this replaces are dropped in place (#1452).
    for key in [
        "uq_send_status_per_space ON _fraiseql_send_status (send_id, tenant_id) NULLS NOT DISTINCT",
        "uq_suppression_per_space ON _fraiseql_suppression (address_hash, tenant_id) NULLS NOT \
         DISTINCT",
    ] {
        assert!(
            ddl.split_whitespace().collect::<Vec<_>>().join(" ").contains(key),
            "missing {key}"
        );
    }
    assert!(!ddl.contains("COALESCE"), "no expression key is left: {ddl}");
    for old in ["uq_send_status_tenant_send", "uq_suppression_tenant_addr"] {
        assert!(
            ddl.contains(&format!("DROP INDEX IF EXISTS {old};")),
            "{old} is dropped in place"
        );
    }
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

/// The delivery-feedback tables' policies are `ENABLE`d, not `FORCE`d, so their owner skips
/// them and a read as the owner proves nothing. A `NOBYPASSRLS` role that owns nothing reads
/// a tenant's rows only with that tenant set, and with none set, only the rows that belong to
/// no tenant.
/// The test builds its own database: a boot skips row-level security and policies it finds
/// already correct, so on a shared one it would read what an earlier run left.
#[tokio::test]
async fn send_tracking_rows_are_read_through_the_tenant_policy() {
    use std::str::FromStr;

    use sqlx::postgres::{PgConnectOptions, PgPoolOptions};

    const ROLE: &str = "fraiseql_send_tracking_reader";
    const PASSWORD: &str = "send_tracking_rls_test_password";

    let Some(db) = fraiseql_test_support::boot_locks::fresh_database("send_rls").await else {
        eprintln!("SKIP send_tracking_rows_are_read_through_the_tenant_policy: no postgres");
        return;
    };
    let pool = db.pool().await;
    execute_ddl(&pool, send_tracking_migration_sql()).await;
    sqlx::query("TRUNCATE _fraiseql_send_status, _fraiseql_suppression")
        .execute(&pool)
        .await
        .unwrap();
    for tenant in [Some("acme"), Some("globex"), None] {
        sqlx::query(
            "INSERT INTO _fraiseql_send_status (send_id, tenant_id, recipient, sending_address, \
             status) VALUES (gen_random_uuid()::text, $1, 'a@example.com', 'noreply@example.com', \
             'sent')",
        )
        .bind(tenant)
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO _fraiseql_suppression (tenant_id, address_hash, reason) \
             VALUES ($1, 'h', 'bounce')",
        )
        .bind(tenant)
        .execute(&pool)
        .await
        .unwrap();
    }
    for statement in [
        format!(
            "DO $$ BEGIN
                 IF NOT EXISTS (SELECT FROM pg_roles WHERE rolname = '{ROLE}') THEN
                     CREATE ROLE {ROLE} LOGIN PASSWORD '{PASSWORD}' NOSUPERUSER NOBYPASSRLS;
                 END IF;
             END $$"
        ),
        format!("ALTER ROLE {ROLE} NOSUPERUSER NOBYPASSRLS LOGIN PASSWORD '{PASSWORD}'"),
        format!("GRANT SELECT ON _fraiseql_send_status, _fraiseql_suppression TO {ROLE}"),
    ] {
        // Reason: test SQL built from a role name this test defines.
        sqlx::query(sqlx::AssertSqlSafe(statement)).execute(&pool).await.unwrap();
    }
    let options = PgConnectOptions::from_str(&db.url()).unwrap().username(ROLE).password(PASSWORD);

    for table in ["_fraiseql_send_status", "_fraiseql_suppression"] {
        // A fresh session per table: a transaction-local setting leaves `''` behind, not unset.
        let reader = PgPoolOptions::new()
            .max_connections(1)
            .connect_with(options.clone())
            .await
            .unwrap();
        let count = format!("SELECT count(*) FROM {table}");
        // Reason: test SQL built from the table names above.
        let (unset,): (i64,) = sqlx::query_as(sqlx::AssertSqlSafe(count.clone()))
            .fetch_one(&reader)
            .await
            .unwrap();
        let mut tx = reader.begin().await.unwrap();
        sqlx::query("SELECT set_config('fraiseql.tenant_id', 'acme', true)")
            .execute(&mut *tx)
            .await
            .unwrap();
        // Reason: test SQL built from the table names above.
        let (acme,): (i64,) = sqlx::query_as(sqlx::AssertSqlSafe(count.clone()))
            .fetch_one(&mut *tx)
            .await
            .unwrap();
        tx.commit().await.unwrap();
        // Reason: test SQL built from the table names above.
        let (owner,): (i64,) =
            sqlx::query_as(sqlx::AssertSqlSafe(count)).fetch_one(&pool).await.unwrap();

        assert_eq!(unset, 1, "{table}: no tenant set reads only the row of no tenant");
        assert_eq!(acme, 1, "{table}: acme reads its own row, not globex's or the unowned one");
        assert_eq!(owner, 3, "{table}: control, the owner reads all three");
    }
}

/// Verify the function-DLQ DDL is syntactically complete and contains the columns,
/// the stable-id unique key, and the Trinity PK. Runs without a database.
#[test]
fn test_dlq_migration_ddl_is_valid_sql() {
    let ddl = dlq_migration_sql();

    assert!(
        ddl.contains("_fraiseql_function_dlq"),
        "DDL must create _fraiseql_function_dlq table"
    );
    assert!(ddl.contains("IF NOT EXISTS"), "DDL must use IF NOT EXISTS");

    for col in [
        "pk_function_dlq",
        "id",
        "source",
        "function_name",
        "trigger_type",
        "idempotency_token",
        "payload",
        "error_message",
        "attempts",
        "created_at",
    ] {
        assert!(ddl.contains(col), "DDL must contain column: {col}");
    }

    // The dead-letter id is the stable handle for inspect/replay — must be unique.
    assert!(
        ddl.contains("uq_function_dlq_id"),
        "DDL must create a unique index on the dead-letter id"
    );
    assert!(ddl.contains("idx_function_dlq_created"), "DDL must create a created_at index");
    assert!(
        ddl.contains("GENERATED ALWAYS AS IDENTITY"),
        "pk must use GENERATED ALWAYS AS IDENTITY (Trinity pattern)"
    );
}

/// Verify the function-DLQ migration creates the table in a real PostgreSQL
/// database and is idempotent (re-running does not error).
#[tokio::test]
async fn test_dlq_migration_creates_table_idempotently() {
    let Some((pool, _svc)) = connect_pool().await else {
        eprintln!(
            "SKIP test_dlq_migration_creates_table_idempotently: no postgres (set DATABASE_URL or enable fraiseql-test-support/local-testcontainers)"
        );
        return;
    };

    let ddl = dlq_migration_sql();
    // Run twice — the second run must not error (IF NOT EXISTS idempotency).
    execute_ddl(&pool, ddl).await;
    execute_ddl(&pool, ddl).await;

    let (exists,): (bool,) =
        sqlx::query_as("SELECT EXISTS (SELECT 1 FROM pg_class WHERE relname = $1)")
            .bind("_fraiseql_function_dlq")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert!(exists, "table _fraiseql_function_dlq must exist after migration");
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
