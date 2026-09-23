//! Tests for the migration advisory lock.
//!
//! The pure test pins the composed script's shape; the DB-backed test reproduces the
//! two races the lock exists to close and is the only one that can tell a lock that is
//! taken from a lock that is released again immediately.

#![allow(clippy::unwrap_used, clippy::panic)] // Reason: test code — fail loud.
#![allow(clippy::print_stderr)] // Reason: skip message when no backing Postgres is available.

use sqlx::PgPool;

use super::{MIGRATION_LOCK_KEY, lock_prefixed, run_migration};

/// A script with the same shape as the six real ones — `IF NOT EXISTS` DDL, then the
/// `ALTER TABLE` / `CREATE POLICY` pair that takes `ACCESS EXCLUSIVE` — on a table no
/// other suite touches.
///
/// Its own table, deliberately. Reproducing the race on a shared `_fraiseql_*` table
/// would mean dropping one while another module's tests read it, which is the
/// interference this whole change is about.
const PROBE_DDL: &str = "\
CREATE TABLE IF NOT EXISTS _fraiseql_migration_lock_probe (
    pk_probe  BIGINT      GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    tenant_id TEXT,
    label     TEXT        NOT NULL,
    seen_at   TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE UNIQUE INDEX IF NOT EXISTS uq_migration_lock_probe_label
    ON _fraiseql_migration_lock_probe (COALESCE(tenant_id, ''), label);

CREATE INDEX IF NOT EXISTS idx_migration_lock_probe_seen_at
    ON _fraiseql_migration_lock_probe (seen_at);

ALTER TABLE _fraiseql_migration_lock_probe ENABLE ROW LEVEL SECURITY;
DROP POLICY IF EXISTS p_migration_lock_probe_tenant ON _fraiseql_migration_lock_probe;
CREATE POLICY p_migration_lock_probe_tenant ON _fraiseql_migration_lock_probe
    USING (tenant_id IS NOT DISTINCT FROM current_setting('fraiseql.tenant_id', true));
";

/// How many runners race. Four is what the psql reproduction used, where three of the
/// four lost on the catalogue index in ten rounds out of ten.
const RACERS: usize = 4;

/// Connect to the harness Postgres (Dagger-bound in CI; a local spawn with the
/// `local-testcontainers` feature); `None` → the test skips cleanly.
async fn connect_pool() -> Option<(PgPool, fraiseql_test_support::Service)> {
    let svc = fraiseql_test_support::postgres().await?;
    let pool = PgPool::connect(svc.url()).await.unwrap();
    Some((pool, svc))
}

/// Run `PROBE_DDL` from `RACERS` tasks at once; return every error raised.
async fn race(pool: &PgPool) -> Vec<String> {
    let mut set = tokio::task::JoinSet::new();
    for _ in 0..RACERS {
        let pool = pool.clone();
        set.spawn(async move { run_migration(&pool, PROBE_DDL).await });
    }
    let mut errors = Vec::new();
    while let Some(joined) = set.join_next().await {
        if let Err(error) = joined.unwrap() {
            errors.push(error.to_string());
        }
    }
    errors
}

#[test]
fn the_lock_is_the_scripts_first_statement() {
    let script = lock_prefixed("CREATE TABLE IF NOT EXISTS t (a INT);");

    // First statement, not merely present: a lock taken after the DDL it is supposed to
    // guard is a lock taken after the race it is supposed to prevent.
    let first = script.split_once(';').expect("the composed script is statement-terminated").0;
    assert_eq!(
        first,
        format!("SELECT pg_advisory_xact_lock({MIGRATION_LOCK_KEY})"),
        "the advisory lock must be the script's first statement, carrying the shared key"
    );

    // The caller's DDL travels through unaltered — the constants are public API and the
    // e2e suites apply them directly.
    assert!(script.ends_with("CREATE TABLE IF NOT EXISTS t (a INT);"));
}

/// The two measured races, in the two states a real startup finds the database in.
///
/// **Cold**, with the table absent: `CREATE TABLE IF NOT EXISTS` checks the catalogue
/// and then creates, so concurrent runners all pass the check and all but one lose on
/// `pg_class_relname_nsp_index` / `pg_type_typname_nsp_index`.
///
/// **Warm**, with the table present: the `IF NOT EXISTS` statements are no-ops, the
/// `ALTER TABLE` / `CREATE POLICY` pair takes `ACCESS EXCLUSIVE`, and two runners
/// deadlock on the same relation.
///
/// Both phases are asserted because the lock has to cover both, and because a fix
/// verified only against a warm database passes on any CI that provisions a fresh one.
#[tokio::test]
async fn concurrent_migrations_race_on_neither_the_catalogue_nor_a_relation_lock() {
    let Some((pool, _svc)) = connect_pool().await else {
        eprintln!(
            "SKIP concurrent_migrations_race_on_neither_the_catalogue_nor_a_relation_lock: no \
             postgres (set DATABASE_URL or enable fraiseql-test-support/local-testcontainers)"
        );
        return;
    };

    sqlx::query("DROP TABLE IF EXISTS _fraiseql_migration_lock_probe CASCADE")
        .execute(&pool)
        .await
        .unwrap();

    let cold = race(&pool).await;
    assert!(
        cold.is_empty(),
        "a cold start must not race on the catalogue; {} of {RACERS} runners failed: {cold:?}",
        cold.len()
    );

    let warm = race(&pool).await;
    assert!(
        warm.is_empty(),
        "a warm start must not deadlock on the relation lock; {} of {RACERS} runners failed: \
         {warm:?}",
        warm.len()
    );
}
