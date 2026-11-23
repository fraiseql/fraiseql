//! Live-PostgreSQL tests for the #573 source coordination primitives: the durable
//! [`PostgresSourceCursorStore`] and the single-firing [`LeaseGuardedRunner`].
//!
//! Proves the parts only a real database can:
//!
//! * **compare-and-swap advance** — a first advance inserts at version 1; each subsequent advance
//!   from the current snapshot bumps the version; a *stale* advance (wrong version) is rejected and
//!   never regresses the watermark;
//! * **transactional advance** — `advance_in_tx` commits or rolls back atomically with the caller's
//!   transaction (Model A's no-reprocess guarantee);
//! * **deny-by-default RLS** — a NOBYPASSRLS role with no `fraiseql.tenant_id` GUC reads zero
//!   cursor rows (mirrors `rls_isolation.rs`);
//! * **single-firing** — two runners (two replicas) on one source: one runs, the other skips,
//!   counting `skips_not_leader`.
//!
//! Self-skips when no Postgres is available (no `#[ignore]`), so it is inert in the
//! database-free `test` leg and runs in the Dagger `integration: observers` suite.
//! Each test uses a fresh random source name so runs never collide on cursor rows.
//!
//! **Execution engine:** PostgreSQL · **Infrastructure:** `DATABASE_URL` ·
//! **Parallelism:** creates a shared subordinate role → run `--test-threads=1`.
#![allow(clippy::unwrap_used)] // Reason: integration test file — panics are acceptable
#![allow(clippy::panic)] // Reason: integration test file
#![allow(clippy::print_stderr)] // Reason: skip diagnostic when no backing Postgres

use std::str::FromStr;

use fraiseql_observers::{
    CursorSnapshot, LeaseGuardedRunner, PostgresSourceCursorStore, RunOutcome, SourceCursorStore,
};
use serde_json::json;
use sqlx::{
    PgPool,
    postgres::{PgConnectOptions, PgPoolOptions},
};

/// NOBYPASSRLS reader — the RLS-subject role the deny-by-default policy must bind.
const RLS_ROLE: &str = "fraiseql_source_cursor_rls_reader";
const ROLE_PASSWORD: &str = "source_cursor_rls_test_password";

/// Connect a pool to the harness-provided Postgres, or `None` to skip cleanly.
async fn connect_pool() -> Option<(PgPool, fraiseql_test_support::Service)> {
    let svc = fraiseql_test_support::postgres().await?;
    let pool = PgPool::connect(svc.url()).await.unwrap();
    Some((pool, svc))
}

/// A pool connected as `role` — same host/port/database as the given URL, with the
/// credentials swapped (mirrors `rls_isolation.rs::role_pool`).
async fn role_pool(url: &str, role: &str) -> PgPool {
    let opts = PgConnectOptions::from_str(url)
        .expect("parse DATABASE_URL")
        .username(role)
        .password(ROLE_PASSWORD);
    PgPoolOptions::new()
        .max_connections(2)
        .connect_with(opts)
        .await
        .unwrap_or_else(|e| panic!("connect as {role}: {e}"))
}

/// A fresh source name so parallel/repeat runs never share a cursor row.
fn unique_source() -> String {
    format!("test-src-{}", uuid::Uuid::new_v4())
}

#[tokio::test]
async fn load_missing_is_the_empty_snapshot() {
    let Some((pool, _svc)) = connect_pool().await else {
        eprintln!("SKIP load_missing_is_the_empty_snapshot: no postgres");
        return;
    };
    let store = PostgresSourceCursorStore::new(pool);
    store.init().await.unwrap();

    let snap = store.load(&unique_source()).await.unwrap();
    assert_eq!(snap, CursorSnapshot::empty(), "an unseen source loads as empty");
    assert!(snap.is_unset());
}

#[tokio::test]
async fn advances_are_monotonic_and_readable() {
    let Some((pool, _svc)) = connect_pool().await else {
        eprintln!("SKIP advances_are_monotonic_and_readable: no postgres");
        return;
    };
    let store = PostgresSourceCursorStore::new(pool);
    store.init().await.unwrap();
    let source = unique_source();

    // First advance inserts at version 1.
    let empty = store.load(&source).await.unwrap();
    assert!(store.advance(&source, &empty, json!({"last_uid": 100})).await.unwrap());
    let v1 = store.load(&source).await.unwrap();
    assert_eq!(v1.value, Some(json!({"last_uid": 100})));
    assert_eq!(v1.version, 1);

    // A second advance from the current snapshot bumps to version 2.
    assert!(store.advance(&source, &v1, json!({"last_uid": 250})).await.unwrap());
    let v2 = store.load(&source).await.unwrap();
    assert_eq!(v2.value, Some(json!({"last_uid": 250})));
    assert_eq!(v2.version, 2);
}

#[tokio::test]
async fn stale_advance_is_rejected_and_does_not_regress() {
    let Some((pool, _svc)) = connect_pool().await else {
        eprintln!("SKIP stale_advance_is_rejected_and_does_not_regress: no postgres");
        return;
    };
    let store = PostgresSourceCursorStore::new(pool);
    store.init().await.unwrap();
    let source = unique_source();

    let empty = store.load(&source).await.unwrap();
    assert!(store.advance(&source, &empty, json!("first")).await.unwrap()); // → v1

    // A stale writer still holding the empty (v0) snapshot: the first-write INSERT
    // loses on ON CONFLICT, so the advance is rejected.
    assert!(
        !store.advance(&source, &empty, json!("stale-from-v0")).await.unwrap(),
        "a v0 advance against an existing row must be rejected"
    );

    let v1 = store.load(&source).await.unwrap();
    assert!(store.advance(&source, &v1, json!("second")).await.unwrap()); // → v2

    // A stale writer still holding the v1 snapshot: the UPDATE matches no row.
    assert!(
        !store.advance(&source, &v1, json!("stale-from-v1")).await.unwrap(),
        "a v1 advance against a v2 row must be rejected"
    );

    // The watermark is exactly the last winning advance — never regressed.
    let current = store.load(&source).await.unwrap();
    assert_eq!(current.value, Some(json!("second")));
    assert_eq!(current.version, 2);
}

#[tokio::test]
async fn advance_in_tx_is_atomic_with_the_caller_transaction() {
    let Some((pool, _svc)) = connect_pool().await else {
        eprintln!("SKIP advance_in_tx_is_atomic_with_the_caller_transaction: no postgres");
        return;
    };
    let store = PostgresSourceCursorStore::new(pool.clone());
    store.init().await.unwrap();
    let source = unique_source();
    let empty = store.load(&source).await.unwrap();

    // A rolled-back transaction leaves no watermark (re-run is safe).
    let mut tx = pool.begin().await.unwrap();
    assert!(store.advance_in_tx(&mut tx, &source, &empty, json!({"n": 5})).await.unwrap());
    tx.rollback().await.unwrap();
    assert_eq!(
        store.load(&source).await.unwrap(),
        CursorSnapshot::empty(),
        "a rolled-back advance must leave no watermark"
    );

    // A committed transaction persists the watermark.
    let mut tx = pool.begin().await.unwrap();
    assert!(store.advance_in_tx(&mut tx, &source, &empty, json!({"n": 9})).await.unwrap());
    tx.commit().await.unwrap();
    let snap = store.load(&source).await.unwrap();
    assert_eq!(snap.value, Some(json!({"n": 9})));
    assert_eq!(snap.version, 1);
}

/// Assert `pool`'s role is refused `INSERT` and `UPDATE` on `table` with
/// `insufficient_privilege` (42501), with `fraiseql.tenant_id` set to `tenant`, the tenant of
/// a row the role reads. The table's write policies are `true`: the grants, not row-level
/// security, keep a role other than the trusted writer out, and with the row visible the
/// refusal can only be the missing grant.
async fn writes_are_refused(pool: &PgPool, table: &str, tenant: &str) {
    for statement in [
        format!("INSERT INTO {table} DEFAULT VALUES"),
        format!("UPDATE {table} SET tenant_id = tenant_id"),
    ] {
        let mut tx = pool.begin().await.unwrap();
        sqlx::query("SELECT set_config('fraiseql.tenant_id', $1, true)")
            .bind(tenant)
            .execute(&mut *tx)
            .await
            .unwrap();
        // Reason: test SQL built from a table name this test defines.
        let result = sqlx::query(sqlx::AssertSqlSafe(statement.clone())).execute(&mut *tx).await;
        let code = result
            .as_ref()
            .err()
            .and_then(|e| e.as_database_error())
            .and_then(|e| e.code())
            .map(|code| code.into_owned());
        assert_eq!(
            code.as_deref(),
            Some("42501"),
            "`{statement}` by a role granted only SELECT must be refused for want of a \
             privilege: {result:?}"
        );
        tx.rollback().await.unwrap();
    }
}

#[tokio::test]
async fn rls_denies_reads_to_a_role_without_the_tenant_guc() {
    let Some((admin, svc)) = connect_pool().await else {
        eprintln!("SKIP rls_denies_reads_to_a_role_without_the_tenant_guc: no postgres");
        return;
    };

    // Create the NOBYPASSRLS reader (idempotent), re-asserting its attributes in
    // case it survived an earlier run.
    // Reason: test SQL built from names and values this test defines.
    sqlx::query(sqlx::AssertSqlSafe(format!(
        "DO $$ BEGIN
             IF NOT EXISTS (SELECT FROM pg_roles WHERE rolname = '{RLS_ROLE}') THEN
                 CREATE ROLE {RLS_ROLE} LOGIN PASSWORD '{ROLE_PASSWORD}' NOSUPERUSER NOBYPASSRLS;
             END IF;
         END $$"
    )))
    .execute(&admin)
    .await
    .unwrap();
    // Reason: test SQL built from names and values this test defines.
    sqlx::query(sqlx::AssertSqlSafe(format!(
        "ALTER ROLE {RLS_ROLE} NOSUPERUSER NOBYPASSRLS LOGIN PASSWORD '{ROLE_PASSWORD}'"
    )))
    .execute(&admin)
    .await
    .unwrap();

    // The table this init creates, not one an earlier run left: row-level security and
    // its policies must be what the migration sets on it, whatever else the database holds.
    sqlx::raw_sql("DROP TABLE IF EXISTS core._fraiseql_source_cursor")
        .execute(&admin)
        .await
        .unwrap();
    let store = PostgresSourceCursorStore::new(admin.clone());
    store.init().await.unwrap();
    // Reason: test SQL built from names and values this test defines.
    sqlx::raw_sql(sqlx::AssertSqlSafe(format!(
        "GRANT USAGE ON SCHEMA core TO {RLS_ROLE}; \
         GRANT SELECT ON core._fraiseql_source_cursor TO {RLS_ROLE}"
    )))
    .execute(&admin)
    .await
    .unwrap();

    // Seed a (global, tenant_id NULL) cursor as the superuser.
    let source = unique_source();
    let empty = store.load(&source).await.unwrap();
    assert!(store.advance(&source, &empty, json!({"secret": true})).await.unwrap());

    // The NOBYPASSRLS reader, with no tenant GUC, sees zero rows (deny-by-default).
    let reader = role_pool(svc.url(), RLS_ROLE).await;
    let (denied,): (i64,) =
        sqlx::query_as("SELECT count(*) FROM core._fraiseql_source_cursor WHERE source_name = $1")
            .bind(&source)
            .fetch_one(&reader)
            .await
            .unwrap();
    assert_eq!(denied, 0, "a NOBYPASSRLS role with no tenant GUC must read zero cursor rows");

    // The superuser (bypasses RLS) still sees the row — proving the deny is RLS,
    // not a missing row.
    let (seen,): (i64,) =
        sqlx::query_as("SELECT count(*) FROM core._fraiseql_source_cursor WHERE source_name = $1")
            .bind(&source)
            .fetch_one(&admin)
            .await
            .unwrap();
    assert_eq!(seen, 1, "the trusted superuser reads the seeded cursor row");

    // A tenant-stamped cursor the reader does read with its tenant set, and still writes
    // nothing: writes are the trusted scheduler's.
    let tenant = "7777777a-7777-4777-8777-777777777777";
    sqlx::query(
        "INSERT INTO core._fraiseql_source_cursor (source_name, tenant_id) VALUES ($1, $2::uuid)",
    )
    .bind(unique_source())
    .bind(tenant)
    .execute(&admin)
    .await
    .unwrap();
    let mut tx = reader.begin().await.unwrap();
    sqlx::query("SELECT set_config('fraiseql.tenant_id', $1, true)")
        .bind(tenant)
        .execute(&mut *tx)
        .await
        .unwrap();
    let (own,): (i64,) = sqlx::query_as("SELECT count(*) FROM core._fraiseql_source_cursor")
        .fetch_one(&mut *tx)
        .await
        .unwrap();
    tx.rollback().await.unwrap();
    assert_eq!(own, 1, "with its tenant set, the reader reads that tenant's cursor");
    writes_are_refused(&reader, "core._fraiseql_source_cursor", tenant).await;
}

#[tokio::test]
async fn two_runners_on_one_source_fire_once() {
    let Some((pool_a, svc)) = connect_pool().await else {
        eprintln!("SKIP two_runners_on_one_source_fire_once: no postgres");
        return;
    };
    // A second pool models a second replica with its own sessions.
    let pool_b = PgPool::connect(svc.url()).await.unwrap();
    let source = unique_source();

    let runner_a = LeaseGuardedRunner::postgres(pool_a, source.clone());
    let runner_b = LeaseGuardedRunner::postgres(pool_b, source);

    // Replica A grabs the lease and holds it until told to release; replica B must
    // attempt while A holds it, so the barrier makes the race deterministic.
    let (holding_tx, holding_rx) = tokio::sync::oneshot::channel::<()>();
    let (release_tx, release_rx) = tokio::sync::oneshot::channel::<()>();
    let a = tokio::spawn(async move {
        runner_a
            .run(|| async move {
                holding_tx.send(()).unwrap();
                release_rx.await.unwrap();
                "a-ran"
            })
            .await
    });

    // Wait until A holds the lease, then B attempts and must skip.
    holding_rx.await.unwrap();
    let b_outcome = runner_b.run(|| async { "b-ran" }).await.unwrap();
    assert_eq!(b_outcome, RunOutcome::SkippedNotLeader, "the second replica must skip");
    assert_eq!(runner_b.skips_not_leader(), 1);

    // Let A finish; it ran exactly once.
    release_tx.send(()).unwrap();
    let a_outcome = a.await.unwrap().unwrap();
    assert_eq!(a_outcome, RunOutcome::Ran("a-ran"));
}

/// A schema that leads the search path ahead of `core`, created empty per test.
const APP_FIRST: &str = "app_first_cursor_home";

/// A pool on the same database whose every connection resolves names through
/// `search_path`.
async fn pool_with_search_path(url: &str, search_path: &str) -> PgPool {
    let options = PgConnectOptions::from_str(url).unwrap().options([("search_path", search_path)]);
    PgPoolOptions::new().max_connections(2).connect_with(options).await.unwrap()
}

/// A cursor the store advanced is the one it resumes from when the pool's search path
/// starts with another schema and leaves `core` off: provisioning there must not create a
/// second, empty table, and the store's reads and writes must reach `core`'s.
#[tokio::test]
async fn a_stored_cursor_is_resumed_when_the_search_path_starts_elsewhere() {
    let Some((pool, svc)) = connect_pool().await else {
        eprintln!(
            "SKIP a_stored_cursor_is_resumed_when_the_search_path_starts_elsewhere: no postgres"
        );
        return;
    };
    // Reason: test SQL built from names and values this test defines.
    sqlx::raw_sql(sqlx::AssertSqlSafe(format!(
        "CREATE SCHEMA IF NOT EXISTS core; DROP SCHEMA IF EXISTS {APP_FIRST} CASCADE"
    )))
    .execute(&pool)
    .await
    .unwrap();
    let (resumed_source, new_source) = (unique_source(), unique_source());
    let core_first = pool_with_search_path(svc.url(), "core").await;
    let first_boot = PostgresSourceCursorStore::new(core_first.clone());
    first_boot.init().await.unwrap();
    let empty = first_boot.load(&resumed_source).await.unwrap();
    assert!(first_boot.advance(&resumed_source, &empty, json!({"page": 7})).await.unwrap());

    // Reason: test SQL built from names and values this test defines.
    sqlx::raw_sql(sqlx::AssertSqlSafe(format!("CREATE SCHEMA {APP_FIRST}")))
        .execute(&pool)
        .await
        .unwrap();
    let app_first = pool_with_search_path(svc.url(), &format!("{APP_FIRST},public")).await;
    let next_boot = PostgresSourceCursorStore::new(app_first);
    next_boot.init().await.unwrap();
    let resumed = next_boot.load(&resumed_source).await.unwrap();
    let updated = next_boot.advance(&resumed_source, &resumed, json!({"page": 8})).await;
    let inserted = next_boot
        .advance(&new_source, &CursorSnapshot::empty(), json!({"page": 1}))
        .await;
    let (shadow,): (Option<String>,) =
        // Reason: test SQL built from names and values this test defines.
        sqlx::query_as(sqlx::AssertSqlSafe(format!("SELECT to_regclass('{APP_FIRST}._fraiseql_source_cursor')::text")))
            .fetch_one(&pool)
            .await
            .unwrap();
    // Reason: test SQL built from names and values this test defines.
    sqlx::raw_sql(sqlx::AssertSqlSafe(format!("DROP SCHEMA IF EXISTS {APP_FIRST} CASCADE")))
        .execute(&pool)
        .await
        .unwrap();

    assert_eq!(resumed.value, Some(json!({"page": 7})), "the stored cursor is resumed");
    assert_eq!(shadow, None, "provisioning created a cursor table in {APP_FIRST}");
    assert!(updated.unwrap(), "the resumed cursor advances");
    assert!(inserted.unwrap(), "a new source's first cursor is written");
    let in_core = PostgresSourceCursorStore::new(core_first);
    assert_eq!(in_core.load(&resumed_source).await.unwrap().value, Some(json!({"page": 8})));
    assert_eq!(in_core.load(&new_source).await.unwrap().value, Some(json!({"page": 1})));
}

/// How many `init` calls race, on how many fresh databases. One database with two
/// parallel callers lost the race in 8 runs of 10; four callers on one lost in 3 of 10.
/// Eight callers on each of five databases makes a run that misses the race unlikely.
const RACERS: usize = 8;
const ROUNDS: usize = 5;

/// Run `init` from one caller per pool, all at once: each pool holds one connection,
/// opened before any caller starts, so none is still connecting while the others run.
/// Return every error raised.
async fn race_init(pools: &[PgPool]) -> Vec<String> {
    let start = std::sync::Arc::new(tokio::sync::Barrier::new(pools.len()));
    let mut set = tokio::task::JoinSet::new();
    for pool in pools {
        let store = PostgresSourceCursorStore::new(pool.clone());
        let start = std::sync::Arc::clone(&start);
        set.spawn(async move {
            start.wait().await;
            store.init().await
        });
    }
    let mut errors = Vec::new();
    while let Some(joined) = set.join_next().await {
        if let Err(error) = joined.unwrap() {
            errors.push(error.to_string());
        }
    }
    errors
}

/// `init` is safe to run concurrently, on a database that has never held the table and
/// on one that has.
///
/// Cold, `CREATE SCHEMA IF NOT EXISTS` and `CREATE TABLE IF NOT EXISTS` check the
/// catalogue and then create, so concurrent callers both pass the check and one loses
/// on a catalogue unique index. Warm, the `ALTER TABLE` and `CREATE POLICY` statements
/// take `ACCESS EXCLUSIVE` on the table, and two callers can deadlock on it.
///
/// The cold phase needs a database without the `core` schema, so each round makes its
/// own and drops it after.
#[tokio::test]
async fn concurrent_init_succeeds_on_a_cold_and_a_warm_database() {
    let Some((admin, svc)) = connect_pool().await else {
        eprintln!("SKIP concurrent_init_succeeds_on_a_cold_and_a_warm_database: no postgres");
        return;
    };
    let mut failures = Vec::new();
    for round in 0..ROUNDS {
        let database = format!("fq_cursor_init_{}", uuid::Uuid::new_v4().simple());
        // Reason: a database name this test generated from a UUID, not caller input.
        sqlx::raw_sql(sqlx::AssertSqlSafe(format!("CREATE DATABASE {database}")))
            .execute(&admin)
            .await
            .unwrap();
        let opts = PgConnectOptions::from_str(svc.url())
            .expect("parse DATABASE_URL")
            .database(&database);
        let mut pools = Vec::with_capacity(RACERS);
        for _ in 0..RACERS {
            pools.push(
                PgPoolOptions::new()
                    .min_connections(1)
                    .max_connections(1)
                    .connect_with(opts.clone())
                    .await
                    .unwrap(),
            );
        }

        for phase in ["cold", "warm"] {
            let errors = race_init(&pools).await;
            if !errors.is_empty() {
                failures.push(format!("round {round}, {phase}: {errors:?}"));
            }
        }
        for pool in pools {
            pool.close().await;
        }
        // Reason: as above.
        sqlx::raw_sql(sqlx::AssertSqlSafe(format!("DROP DATABASE {database}")))
            .execute(&admin)
            .await
            .unwrap();
    }
    assert!(failures.is_empty(), "concurrent inits failed: {failures:#?}");
}
