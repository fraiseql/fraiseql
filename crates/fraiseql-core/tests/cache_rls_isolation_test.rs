//! Cache + RLS isolation integration tests, and the #762 gate for
//! `validate_rls_active`.
//!
//! Two things are asserted here. First, that two tenants issuing the same query
//! against an RLS-protected fixture get independently cached results. Second — and
//! this is what #762 is about — that `validate_rls_active` actually *checks*
//! something: it used to read the `row_security` GUC, which PostgreSQL defaults to
//! `on` whether or not a single policy exists, so the documented "refuse startup if
//! RLS appears inactive" gate returned `Ok(())` on a stock database with no RLS
//! anywhere.
//!
//! ## Why this file never ran
//!
//! It was gated on `TEST_DATABASE_URL`. The `integration: postgres` leg — the one
//! that runs `cargo test -p fraiseql-core --test '*'` — sets only `DATABASE_URL`,
//! so every test here self-skipped in CI, which is how
//! `test_validate_rls_active_fails_without_rls` could ship accepting *either*
//! outcome ("we assert the return type is correct either way") without anyone
//! noticing it proved nothing. It now uses the harness's `DATABASE_URL`, like every
//! other PostgreSQL suite.
//!
//! ## Why the fixture view changed
//!
//! `v_tenant_item` was a plain view. A plain view executes with its *owner's*
//! privileges and bypasses the caller's RLS policies entirely, so the fixture
//! demonstrating tenant isolation did not have it. It is now
//! `WITH (security_invoker = true)`, which is also what the new check requires.
//!
//! **Execution engine:** `PostgreSQL` · **Infrastructure:** `DATABASE_URL` ·
//! **Parallelism:** creates and drops shared `tenant_items` / `v_tenant_item`
//! fixtures → run `--test-threads=1`.

#![cfg(test)]
#![allow(clippy::manual_let_else, clippy::print_stdout, clippy::print_stderr)] // Reason: test uses match for clarity in assertion context
#![allow(clippy::panic, clippy::unwrap_used)] // Reason: test code, panics acceptable
use fraiseql_core::{
    cache::{CacheConfig, CachedDatabaseAdapter, QueryResultCache, RlsEnforcement},
    db::{DatabaseAdapter, postgres::PostgresAdapter},
    error::FraiseQLError,
    schema::{CompiledSchema, QueryDefinition},
};
use fraiseql_test_support::try_database_url;

/// Returns the test database URL from the harness, or `None` if not set.
fn test_db_url() -> Option<String> {
    try_database_url()
}

/// A compiled schema whose only query reads `v_tenant_item` — the relation the
/// RLS check has to look at. `validate_rls_active` derives its work-list from
/// `sql_source`, so a schema with no queries would vacuously pass.
fn schema_reading(source: &str) -> CompiledSchema {
    let mut schema = CompiledSchema::new();
    schema.queries.push(QueryDefinition {
        name: "items".to_string(),
        return_type: "Item".to_string(),
        sql_source: Some(source.to_string()),
        ..QueryDefinition::default()
    });
    schema
}

/// SQL to set up the RLS-protected test fixture.
///
/// Rows are inserted **before** RLS is switched on: the policy has no `WITH CHECK`,
/// so under `FORCE ROW LEVEL SECURITY` its `USING` expression governs INSERT too,
/// and seeding afterwards would be rejected for want of an `app.tenant_id`.
///
/// The view is `security_invoker` so it honours the *caller's* policies. A plain
/// view runs with its owner's privileges and would return every tenant's rows —
/// which is what this fixture used to do, in the file whose subject is tenant
/// isolation.
const SETUP_SQL: &str = "
    DROP VIEW IF EXISTS v_tenant_item CASCADE;
    DROP TABLE IF EXISTS tenant_items CASCADE;

    CREATE TABLE tenant_items (
        pk_item BIGINT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
        tenant_id UUID NOT NULL,
        name TEXT NOT NULL
    );

    INSERT INTO tenant_items (tenant_id, name)
    SELECT '11111111-1111-1111-1111-111111111111'::uuid, 'item-a-' || i
    FROM generate_series(1, 3) i;

    INSERT INTO tenant_items (tenant_id, name)
    SELECT '22222222-2222-2222-2222-222222222222'::uuid, 'item-b-' || i
    FROM generate_series(1, 5) i;

    CREATE VIEW v_tenant_item WITH (security_invoker = true) AS
        SELECT jsonb_build_object(
            'pk_item', pk_item,
            'tenant_id', tenant_id,
            'name', name
        ) AS data
        FROM tenant_items;

    ALTER TABLE tenant_items ENABLE ROW LEVEL SECURITY;
    ALTER TABLE tenant_items FORCE ROW LEVEL SECURITY;

    DROP POLICY IF EXISTS tenant_isolation ON tenant_items;
    -- NULLIF is not decoration. `set_config(..., true)` is transaction-local, so on a
    -- pooled connection that has served a tenant before, the setting reverts to the
    -- empty string rather than disappearing — and ''::uuid raises 22P02, turning an
    -- unauthenticated read into a 500 instead of an empty result set.
    CREATE POLICY tenant_isolation ON tenant_items
        USING (tenant_id = NULLIF(current_setting('app.tenant_id', true), '')::uuid);

    DO $$ BEGIN
        IF NOT EXISTS (SELECT 1 FROM pg_roles WHERE rolname = 'p04_rls_app') THEN
            CREATE ROLE p04_rls_app LOGIN PASSWORD 'p04_rls_app_pw';
        END IF;
    END $$;
    GRANT USAGE ON SCHEMA public TO p04_rls_app;
    GRANT SELECT ON tenant_items TO p04_rls_app;
    GRANT SELECT ON v_tenant_item TO p04_rls_app;
";

/// The unprivileged role the isolation assertions connect as.
///
/// **This is load-bearing.** The harness's `DATABASE_URL` role is a superuser with
/// `rolbypassrls`, for which PostgreSQL skips every policy — so an RLS test run as
/// that role returns every tenant's rows no matter how correct the policies are, and
/// can only ever pass by asserting something weaker than isolation. The fixture
/// therefore provisions an ordinary `LOGIN` role and reconnects as it.
const APP_ROLE: &str = "p04_rls_app";
const APP_PASSWORD: &str = "p04_rls_app_pw";

/// Rebuild `db_url` with the unprivileged role's credentials, preserving host,
/// port and database.
fn app_role_url(db_url: &str) -> String {
    let cfg: tokio_postgres::Config = db_url.parse().expect("parse DATABASE_URL");
    let host = match cfg.get_hosts().first().expect("a host") {
        tokio_postgres::config::Host::Tcp(h) => h.clone(),
        #[cfg(unix)]
        tokio_postgres::config::Host::Unix(p) => p.display().to_string(),
    };
    let port = cfg.get_ports().first().copied().unwrap_or(5432);
    let dbname = cfg.get_dbname().expect("a database name");
    format!("postgres://{APP_ROLE}:{APP_PASSWORD}@{host}:{port}/{dbname}")
}

const TENANT_A: &str = "11111111-1111-1111-1111-111111111111";
const TENANT_B: &str = "22222222-2222-2222-2222-222222222222";

/// Connect to PostgreSQL via `tokio_postgres` for raw DDL/DML fixture setup.
///
/// Returns the connected client; the caller must keep the connection task alive.
async fn setup_raw_connection(db_url: &str) -> tokio_postgres::Client {
    // tokio-postgres uses the libpq-style connection string when passed as a URL.
    let (client, connection) = tokio_postgres::connect(db_url, tokio_postgres::NoTls)
        .await
        .expect("raw tokio_postgres connection");
    tokio::spawn(async move {
        if let Err(e) = connection.await {
            eprintln!("Setup connection error: {e}");
        }
    });
    client
}

/// Two tenants issuing the *same* query with only their session variable differing
/// must each see only their own rows, and the result cache must never serve one
/// tenant the other's rows.
///
/// The previous version of this test could not make that claim. It set no session
/// variable at all — its own comment said "`CachedDatabaseAdapter` does not expose
/// session variable setting, so we verify isolation indirectly" — and then asserted
/// that two *different* `WHERE` clauses produce different results. That is true of
/// any cache, with or without RLS. It also never ran (see the module docs).
#[tokio::test]
async fn tenant_reads_never_cross_under_rls() {
    let Some(db_url) = test_db_url() else {
        eprintln!("Skipping: DATABASE_URL not set");
        return;
    };

    let setup_client = setup_raw_connection(&db_url).await;
    setup_client.batch_execute(SETUP_SQL).await.expect("setup SQL");

    // Connect as the unprivileged role: the harness role bypasses RLS entirely.
    let adapter = PostgresAdapter::new(&app_role_url(&db_url)).await.expect("PostgresAdapter");
    let cache = QueryResultCache::new(CacheConfig::enabled());
    let cached = CachedDatabaseAdapter::new(adapter, cache, "test-1.0.0".to_string());

    // The gate must agree that this fixture is genuinely RLS-protected before the
    // isolation assertions below mean anything.
    cached.validate_rls_active(&schema_reading("v_tenant_item")).await.expect(
        "the fixture must pass the RLS gate — enable RLS, declare a policy, and make \
         the view security_invoker",
    );

    // Identical query, identical (absent) WHERE clause: the only thing that differs
    // is the tenant session variable the RLS policy reads.
    let read = |tenant: &'static str| {
        let cached = &cached;
        async move {
            cached
                .execute_where_query_arc_with_session(
                    "v_tenant_item",
                    None,
                    None,
                    None,
                    None,
                    &[("app.tenant_id", tenant)],
                    fraiseql_core::db::types::ReadRouting::Any,
                )
                .await
                .expect("tenant read")
        }
    };

    let a = read(TENANT_A).await;
    assert_eq!(a.len(), 3, "tenant A must see exactly its own 3 rows, got {a:?}");

    let b = read(TENANT_B).await;
    assert_eq!(
        b.len(),
        5,
        "tenant B must see exactly its own 5 rows — 3 would mean it was served \
         tenant A's cached response, got {b:?}"
    );

    // Re-read as A after B has populated the cache under the same key shape.
    let a_again = read(TENANT_A).await;
    assert_eq!(
        a_again.len(),
        3,
        "tenant A must still see 3 rows; 5 would mean tenant B's rows came back out \
         of the cache, got {a_again:?}"
    );

    // And with no tenant set at all, the policy matches nothing.
    let anonymous = cached
        .execute_where_query("v_tenant_item", None, None, None, None)
        .await
        .expect("anonymous read");
    assert!(
        anonymous.is_empty(),
        "a read with no tenant session variable must return nothing under RLS, got {anonymous:?}"
    );

    setup_client
        .batch_execute(
            "DROP VIEW IF EXISTS v_tenant_item CASCADE; DROP TABLE IF EXISTS tenant_items CASCADE",
        )
        .await
        .ok();
}

/// #762 core: a database with no RLS must **fail** the gate.
///
/// The previous assertion accepted either outcome, with the comment "we assert the
/// return type is correct either way". On a stock PostgreSQL — `row_security` on,
/// zero policies — the old implementation always took the `Ok` branch, so the test
/// documented the vacuous pass rather than catching it.
#[tokio::test]
async fn validate_rls_active_fails_on_a_table_with_no_policies() {
    let Some(db_url) = test_db_url() else {
        eprintln!("Skipping: DATABASE_URL not set");
        return;
    };
    let setup = setup_raw_connection(&db_url).await;
    setup
        .batch_execute(
            "DROP TABLE IF EXISTS tb_rls_probe CASCADE; \
             CREATE TABLE tb_rls_probe (id int, data jsonb);",
        )
        .await
        .expect("fixture");

    let adapter = PostgresAdapter::new(&db_url).await.expect("PostgresAdapter");
    let cache = QueryResultCache::new(CacheConfig::enabled());
    let cached = CachedDatabaseAdapter::new(adapter, cache, "test".to_string());

    let result = cached.validate_rls_active(&schema_reading("tb_rls_probe")).await;
    setup.batch_execute("DROP TABLE IF EXISTS tb_rls_probe CASCADE").await.ok();

    match result {
        Err(FraiseQLError::Configuration { message }) => {
            assert!(
                message.contains("tb_rls_probe"),
                "the error must name the unprotected relation: {message}"
            );
            assert!(
                message.contains("row level security is not enabled"),
                "the error must say what is wrong, not just that something is: {message}"
            );
        },
        Ok(()) => panic!(
            "#762: a table with no RLS passed the gate. `row_security` defaults to `on`, so \
             reading that GUC reports success on any stock PostgreSQL."
        ),
        Err(other) => panic!("unexpected error type: {other:?}"),
    }
}

/// RLS enabled but no policy is not isolation either: it denies everything to
/// non-owners and is bypassed outright by the table's owner.
#[tokio::test]
async fn validate_rls_active_fails_when_rls_is_on_but_no_policy_exists() {
    let Some(db_url) = test_db_url() else {
        eprintln!("Skipping: DATABASE_URL not set");
        return;
    };
    let setup = setup_raw_connection(&db_url).await;
    setup
        .batch_execute(
            "DROP TABLE IF EXISTS tb_rls_probe CASCADE; \
             CREATE TABLE tb_rls_probe (id int, data jsonb); \
             ALTER TABLE tb_rls_probe ENABLE ROW LEVEL SECURITY;",
        )
        .await
        .expect("fixture");

    let adapter = PostgresAdapter::new(&db_url).await.expect("PostgresAdapter");
    let cache = QueryResultCache::new(CacheConfig::enabled());
    let cached = CachedDatabaseAdapter::new(adapter, cache, "test".to_string());

    let result = cached.validate_rls_active(&schema_reading("tb_rls_probe")).await;
    setup.batch_execute("DROP TABLE IF EXISTS tb_rls_probe CASCADE").await.ok();

    let Err(FraiseQLError::Configuration { message }) = result else {
        panic!("#762: RLS enabled with zero policies must not pass: {result:?}");
    };
    assert!(message.contains("no policy is defined"), "{message}");
}

/// A plain view over an RLS-protected table bypasses the caller's policies, so it
/// must fail even though the underlying table is perfectly protected. This is the
/// case the fixture in this very file used to get wrong.
#[tokio::test]
async fn validate_rls_active_fails_for_a_view_that_is_not_security_invoker() {
    let Some(db_url) = test_db_url() else {
        eprintln!("Skipping: DATABASE_URL not set");
        return;
    };
    let setup = setup_raw_connection(&db_url).await;
    setup
        .batch_execute(
            "DROP VIEW IF EXISTS v_rls_probe CASCADE; \
             DROP TABLE IF EXISTS tb_rls_probe CASCADE; \
             CREATE TABLE tb_rls_probe (id int, tenant_id text, data jsonb); \
             ALTER TABLE tb_rls_probe ENABLE ROW LEVEL SECURITY; \
             CREATE POLICY p ON tb_rls_probe USING (true); \
             CREATE VIEW v_rls_probe AS SELECT data FROM tb_rls_probe;",
        )
        .await
        .expect("fixture");

    let adapter = PostgresAdapter::new(&db_url).await.expect("PostgresAdapter");
    let cache = QueryResultCache::new(CacheConfig::enabled());
    let cached = CachedDatabaseAdapter::new(adapter, cache, "test".to_string());

    let plain = cached.validate_rls_active(&schema_reading("v_rls_probe")).await;

    // The same view, redefined as security_invoker, must pass.
    setup
        .batch_execute(
            "DROP VIEW v_rls_probe; \
             CREATE VIEW v_rls_probe WITH (security_invoker = true) AS \
               SELECT data FROM tb_rls_probe;",
        )
        .await
        .expect("redefine view");
    let invoker = cached.validate_rls_active(&schema_reading("v_rls_probe")).await;

    setup
        .batch_execute("DROP VIEW IF EXISTS v_rls_probe; DROP TABLE IF EXISTS tb_rls_probe CASCADE")
        .await
        .ok();

    let Err(FraiseQLError::Configuration { message }) = plain else {
        panic!("a non-security_invoker view must fail the gate: {plain:?}");
    };
    assert!(message.contains("security_invoker"), "{message}");
    invoker.expect("a security_invoker view over an RLS-protected table must pass");
}

/// A relation that is not there cannot be protected, and must not pass by absence.
#[tokio::test]
async fn validate_rls_active_fails_for_a_missing_relation() {
    let Some(db_url) = test_db_url() else {
        eprintln!("Skipping: DATABASE_URL not set");
        return;
    };
    let adapter = PostgresAdapter::new(&db_url).await.expect("PostgresAdapter");
    let cache = QueryResultCache::new(CacheConfig::enabled());
    let cached = CachedDatabaseAdapter::new(adapter, cache, "test".to_string());

    let Err(FraiseQLError::Configuration { message }) =
        cached.validate_rls_active(&schema_reading("v_does_not_exist_p04")).await
    else {
        panic!("a missing relation must fail the gate");
    };
    assert!(message.contains("does not exist"), "{message}");
}

/// Verifies `enforce_rls()` with `RlsEnforcement::Off` never errors.
#[tokio::test]
async fn test_enforce_rls_off_skips_check() {
    let Some(db_url) = test_db_url() else {
        eprintln!("Skipping: DATABASE_URL not set");
        return;
    };

    let adapter = PostgresAdapter::new(&db_url).await.expect("PostgresAdapter");
    let cache = QueryResultCache::new(CacheConfig::enabled());
    let cached = CachedDatabaseAdapter::new(adapter, cache, "test".to_string());

    // With `Off`, the check is skipped entirely — must always succeed even though
    // the relation does not exist.
    cached
        .enforce_rls(&schema_reading("v_does_not_exist_p04"), RlsEnforcement::Off)
        .await
        .expect("enforce_rls(Off) must never error");
}

/// `Warn` must log and continue; `Error` must propagate. The two used to be
/// indistinguishable because the underlying check never failed.
#[tokio::test]
async fn enforce_rls_warn_tolerates_what_error_refuses() {
    let Some(db_url) = test_db_url() else {
        eprintln!("Skipping: DATABASE_URL not set");
        return;
    };
    let adapter = PostgresAdapter::new(&db_url).await.expect("PostgresAdapter");
    let cache = QueryResultCache::new(CacheConfig::enabled());
    let cached = CachedDatabaseAdapter::new(adapter, cache, "test".to_string());
    let schema = schema_reading("v_does_not_exist_p04");

    cached
        .enforce_rls(&schema, RlsEnforcement::Warn)
        .await
        .expect("warn must not error");
    assert!(
        cached.enforce_rls(&schema, RlsEnforcement::Error).await.is_err(),
        "error enforcement must propagate"
    );
}
