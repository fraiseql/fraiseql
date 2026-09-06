#![allow(clippy::unwrap_used, clippy::print_stderr)] // Reason: test code; panics are acceptable and the skip diagnostic goes to stderr

use sqlx::PgPool;

use super::storage_migration_sql;

/// Connect to the harness Postgres (Dagger-bound in CI; a local spawn with the
/// `local-testcontainers` feature). Returns the pool plus the service guard, which the
/// caller holds for the test.
async fn connect_pool() -> (PgPool, fraiseql_test_support::Service) {
    let svc = fraiseql_test_support::postgres()
        .await
        .expect("DATABASE_URL must be set (or enable fraiseql-test-support/local-testcontainers)");
    let pool = PgPool::connect(svc.url()).await.unwrap();
    (pool, svc)
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

#[test]
fn test_migration_ddl_is_valid_sql() {
    let ddl = storage_migration_sql();

    // Must contain the table name
    assert!(
        ddl.contains("_fraiseql_storage_objects"),
        "DDL must create _fraiseql_storage_objects table"
    );

    // Must use IF NOT EXISTS for idempotency
    assert!(ddl.contains("IF NOT EXISTS"), "DDL must use IF NOT EXISTS");

    // Must have the required columns matching StorageMetadataRow
    for col in [
        "pk_storage_object",
        "bucket",
        "key",
        "content_type",
        "size_bytes",
        "etag",
        "owner_id",
        "created_at",
        "updated_at",
    ] {
        assert!(ddl.contains(col), "DDL must contain column: {col}");
    }

    // Must have indexes
    assert!(
        ddl.contains("idx_storage_objects_bucket_key"),
        "DDL must create bucket+key index"
    );
    assert!(ddl.contains("idx_storage_objects_owner"), "DDL must create owner index");

    // Must follow Trinity pattern for primary key
    assert!(
        ddl.contains("GENERATED ALWAYS AS IDENTITY"),
        "pk must use GENERATED ALWAYS AS IDENTITY (Trinity pattern)"
    );
}

#[tokio::test]
async fn test_migration_creates_table() {
    let (pool, _svc) = connect_pool().await;

    let ddl = storage_migration_sql();
    execute_ddl(&pool, ddl).await;

    // Verify table exists by querying pg_class
    let (exists,): (bool,) = sqlx::query_as(
        "SELECT EXISTS (
            SELECT 1 FROM pg_class WHERE relname = '_fraiseql_storage_objects'
        )",
    )
    .fetch_one(&pool)
    .await
    .unwrap();

    assert!(exists, "table _fraiseql_storage_objects must exist after migration");
}

#[tokio::test]
async fn test_migration_is_idempotent() {
    let (pool, _svc) = connect_pool().await;

    let ddl = storage_migration_sql();

    // Run twice — second run must not error
    execute_ddl(&pool, ddl).await;
    execute_ddl(&pool, ddl).await;
}

/// #1286: several runners migrating the same cold database at once all succeed.
///
/// The DDL carries `IF NOT EXISTS` on every statement, which made it look safe to run from
/// anywhere. PostgreSQL evaluates the existence check and the create separately, so concurrent
/// runners against a database that does not yet carry the objects all observe "absent" and all
/// create; the losers get `23505` on `pg_type_typname_nsp_index` /
/// `pg_class_relname_nsp_index`, or `42P07 relation already exists`.
///
/// **Why this test builds its own database.** The defect self-heals: the first run that fails
/// leaves the objects behind, so every later run against that database passes. A test against
/// the shared harness database would therefore pass on a warm rig whatever the code did — it
/// would be a fixture that agrees with a broken engine. Only a *cold* database can express it,
/// so this creates one, uses it, and drops it.
///
/// Measured before the fix, on this shape: 5 of 7 sibling tests failed. With
/// `run_storage_migration` taking `pg_advisory_xact_lock` in the DDL's own transaction, the
/// runners queue.
/// Multi-threaded deliberately: on the default current-thread runtime the runners would
/// interleave at await points rather than genuinely overlap, and the window this defect lives
/// in is between one session's existence check and its create.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn concurrent_migrations_against_a_cold_database_all_succeed() {
    /// Fixed rather than random: this test owns the name, drops it first, and drops it after.
    const SCRATCH_DB: &str = "fraiseql_storage_migration_race_test";
    /// Enough runners to lose the race reliably. At 2 the losers were intermittent.
    const RUNNERS: usize = 8;

    let Some(svc) = fraiseql_test_support::postgres().await else {
        eprintln!("skipping: DATABASE_URL not set");
        return;
    };

    // Rebuild the URL against the scratch database, preserving any query string.
    let base = svc.url();
    let (prefix, tail) = base.rsplit_once('/').expect("a PostgreSQL URL carries a database path");
    let query = tail.find('?').map_or("", |i| &tail[i..]);
    let scratch_url = format!("{prefix}/{SCRATCH_DB}{query}");

    let admin = PgPool::connect(base).await.unwrap();
    sqlx::query(&format!("DROP DATABASE IF EXISTS {SCRATCH_DB} WITH (FORCE)"))
        .execute(&admin)
        .await
        .unwrap();
    sqlx::query(&format!("CREATE DATABASE {SCRATCH_DB}"))
        .execute(&admin)
        .await
        .unwrap();

    // The pool is built before the runners start, so the race is over the DDL and not over
    // connection setup.
    let pool = PgPool::connect(&scratch_url).await.unwrap();
    // `tokio::join!` rather than `spawn`: a spawned task must be `'static`, and the borrow
    // `sqlx` takes of the transaction inside `run_storage_migration` is not
    // (`implementation of Executor is not general enough`). These are polled concurrently on
    // one task, so each runner's statements are in flight while the others wait on the server
    // — which is the overlap this defect needs. The mutation check is what proves that is
    // enough: removing the lock must redden this test.
    let r = tokio::join!(
        super::run_storage_migration(&pool),
        super::run_storage_migration(&pool),
        super::run_storage_migration(&pool),
        super::run_storage_migration(&pool),
        super::run_storage_migration(&pool),
        super::run_storage_migration(&pool),
        super::run_storage_migration(&pool),
        super::run_storage_migration(&pool),
    );
    // Typed rather than `assert_eq!(len, RUNNERS)`: adding a `join!` arm without bumping
    // `RUNNERS` is then a compile error instead of a runtime one.
    let results: [Result<(), sqlx::Error>; RUNNERS] = r.into();

    let failures: Vec<String> = results
        .iter()
        .filter_map(|r| r.as_ref().err())
        .map(ToString::to_string)
        .collect();

    pool.close().await;
    sqlx::query(&format!("DROP DATABASE IF EXISTS {SCRATCH_DB} WITH (FORCE)"))
        .execute(&admin)
        .await
        .unwrap();

    assert!(
        failures.is_empty(),
        "{} of {RUNNERS} concurrent migrations failed against a cold database; the DDL is \
         serialized by an advisory lock precisely so none can: {failures:?}",
        failures.len()
    );
}
