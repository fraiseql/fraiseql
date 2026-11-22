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
            // Reason: the crate's own migration DDL, split into statements.
            sqlx::query(sqlx::AssertSqlSafe(trimmed)).execute(pool).await.unwrap();
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
    // Reason: `SCRATCH_DB` is a compile-time constant.
    sqlx::query(sqlx::AssertSqlSafe(format!(
        "DROP DATABASE IF EXISTS {SCRATCH_DB} WITH (FORCE)"
    )))
    .execute(&admin)
    .await
    .unwrap();
    // Reason: `SCRATCH_DB` is a compile-time constant.
    sqlx::query(sqlx::AssertSqlSafe(format!("CREATE DATABASE {SCRATCH_DB}")))
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
    // Reason: `SCRATCH_DB` is a compile-time constant.
    sqlx::query(sqlx::AssertSqlSafe(format!(
        "DROP DATABASE IF EXISTS {SCRATCH_DB} WITH (FORCE)"
    )))
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

/// The storage migration and the server's boot runner, which runs the same script under
/// `lock_prefixed`, exclude each other: they take one key. Each runs on a cold database
/// alongside the other, four runners each. Two keys would let a storage runner and a
/// server runner both pass `IF NOT EXISTS`, one losing on a catalogue unique index.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn the_migration_and_the_servers_runner_exclude_each_other() {
    /// Cold databases the race runs on.
    const ROUNDS: usize = 5;

    let Some(svc) = fraiseql_test_support::postgres().await else {
        eprintln!("skipping: DATABASE_URL not set");
        return;
    };
    let base = svc.url();
    let (prefix, tail) = base.rsplit_once('/').expect("a PostgreSQL URL carries a database path");
    let query = tail.find('?').map_or("", |i| &tail[i..]);
    let admin = PgPool::connect(base).await.unwrap();

    // The server's runner, as `fraiseql_server::migration_lock::run_migration` runs it.
    let servers = |pool: PgPool| async move {
        // Reason: the crate's own DDL constant, behind the workspace's fixed lock statement.
        sqlx::raw_sql(sqlx::AssertSqlSafe(fraiseql_guard::ddl::lock_prefixed(
            super::storage_migration_sql(),
        )))
        .execute(&pool)
        .await
        .map(|_| ())
    };

    let mut failures = Vec::new();
    for round in 0..ROUNDS {
        let database = format!("fq_storage_keys_{}", uuid::Uuid::new_v4().simple());
        // Reason: a database name this test generated from a UUID, not caller input.
        sqlx::query(sqlx::AssertSqlSafe(format!("CREATE DATABASE {database}")))
            .execute(&admin)
            .await
            .unwrap();
        let pool = PgPool::connect(&format!("{prefix}/{database}{query}")).await.unwrap();
        let results: [Result<(), sqlx::Error>; 8] = tokio::join!(
            super::run_storage_migration(&pool),
            servers(pool.clone()),
            super::run_storage_migration(&pool),
            servers(pool.clone()),
            super::run_storage_migration(&pool),
            servers(pool.clone()),
            super::run_storage_migration(&pool),
            servers(pool.clone()),
        )
        .into();
        failures.extend(
            results
                .iter()
                .filter_map(|r| r.as_ref().err())
                .map(|e| format!("round {round}: {e}")),
        );
        pool.close().await;
        // Reason: as above.
        sqlx::query(sqlx::AssertSqlSafe(format!("DROP DATABASE {database} WITH (FORCE)")))
            .execute(&admin)
            .await
            .unwrap();
    }
    assert!(failures.is_empty(), "the two runners raced: {failures:#?}");
}

/// #2130: the migration clears an etag that is the object's key (`bucket/key`, or `key`
/// from before v2.6.0) and leaves every other etag alone. A second run changes no row.
#[tokio::test]
async fn the_migration_clears_key_shaped_etags_once_and_only_those() {
    let (pool, _svc) = connect_pool().await;
    super::run_storage_migration(&pool).await.unwrap();
    sqlx::query("DELETE FROM _fraiseql_storage_objects WHERE bucket = 'm2130'")
        .execute(&pool)
        .await
        .unwrap();
    for (key, etag) in [
        ("scoped.txt", Some("m2130/scoped.txt")),
        ("bare.txt", Some("bare.txt")),
        ("hashed.txt", Some("\"0123456789abcdef0123456789abcdef\"")),
        ("other.txt", Some("m2130/not-this-key.txt")),
        ("none.txt", None),
    ] {
        sqlx::query(
            "INSERT INTO _fraiseql_storage_objects (bucket, key, content_type, size_bytes, etag) \
             VALUES ('m2130', $1, 'text/plain', 1, $2)",
        )
        .bind(key)
        .bind(etag)
        .execute(&pool)
        .await
        .unwrap();
    }
    let rows = || async {
        sqlx::query_as::<_, (String, Option<String>, String)>(
            "SELECT key, etag, xmin::text FROM _fraiseql_storage_objects \
             WHERE bucket = 'm2130' ORDER BY key",
        )
        .fetch_all(&pool)
        .await
        .unwrap()
    };

    super::run_storage_migration(&pool).await.unwrap();
    let first = rows().await;
    let etags: Vec<(&str, Option<&str>)> =
        first.iter().map(|(k, e, _)| (k.as_str(), e.as_deref())).collect();
    assert_eq!(
        etags,
        vec![
            ("bare.txt", None),
            ("hashed.txt", Some("\"0123456789abcdef0123456789abcdef\"")),
            ("none.txt", None),
            ("other.txt", Some("m2130/not-this-key.txt")),
            ("scoped.txt", None),
        ]
    );

    super::run_storage_migration(&pool).await.unwrap();
    assert_eq!(rows().await, first, "a second run rewrote a row");

    sqlx::query("DELETE FROM _fraiseql_storage_objects WHERE bucket = 'm2130'")
        .execute(&pool)
        .await
        .unwrap();
}
