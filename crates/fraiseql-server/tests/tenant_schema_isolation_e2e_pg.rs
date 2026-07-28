//! #809 regression: schema-per-tenant isolation must hold on **every** pooled
//! connection, not on the one connection that happened to be checked out at
//! tenant-registration time.
//!
//! `configure_search_path` issued a single session-scoped `SET search_path TO
//! tenant_x, public` through `execute_raw_query`, which borrows one connection from
//! the deadpool pool and returns it. `RecyclingMethod::Fast` means no `DISCARD ALL`,
//! and no `post_create` hook existed, so connections 2..N were opened lazily with the
//! default `search_path` and resolved unqualified relations against `public`.
//!
//! **The concurrency is the whole test.** A single-connection tenant query passes
//! against the broken code — that is why this shipped. Every case here holds several
//! connections open simultaneously (via `pg_sleep`) so the pool is forced to open
//! more than one, and asserts the isolation on all of them.
//!
//! The fixture is deliberately the shape that makes the bug *silent* rather than
//! loud: a decoy relation of the same name exists in `public`, so a connection
//! without the tenant search path returns the wrong rows with no error at all.
//!
//! Self-skips when no `DATABASE_URL` is set (no `#[ignore]`), so it is inert in the
//! database-free `test` leg and runs in the Dagger `integration: server` suite.
//!
//! **Execution engine:** `PostgreSQL` · **Infrastructure:** `DATABASE_URL` ·
//! **Parallelism:** creates and drops its own `tenant_p04*` schemas and a
//! `public.v_iso_probe` decoy → run `--test-threads=1`.
#![allow(clippy::unwrap_used, clippy::panic, clippy::print_stderr)] // Reason: test code — panics and skip diagnostics are acceptable

use std::{collections::HashMap, sync::Arc};

use fraiseql_core::{
    db::postgres::PostgresAdapter, prelude::DatabaseAdapter as _, runtime::Executor,
};
use fraiseql_server::tenancy::{TenantPoolConfig, create_tenant_executor};
use fraiseql_test_support::try_database_url;
use serde_json::Value;

/// Tenant keys used by this suite. `tenant_` is prepended by the isolation module.
const TENANT_A: &str = "p04isoa";
const TENANT_B: &str = "p04isob";

/// Unqualified relation every case reads. It exists in both tenant schemas **and**
/// in `public`, so a connection on the default search path silently reads the decoy.
const PROBE_RELATION: &str = "v_iso_probe";

/// Marker stored in the `public` decoy. Seeing this in a tenant's result set is the
/// #809 leak: the query resolved against `public` instead of `tenant_{key}`.
const PUBLIC_DECOY_MARKER: &str = "PUBLIC-DECOY";

/// Number of queries driven concurrently. Comfortably above the pool's `max_size`
/// so every connection slot is used.
const CONCURRENCY: usize = 8;

/// Pool ceiling for each tenant. Must be > 1 — the whole defect is invisible at 1.
const POOL_MAX: u32 = 4;

/// `application_name` tag used to terminate exactly one pool's backends.
const APP_NAME: &str = "p04_iso_replace";

fn schema_json_for(mode: &str) -> String {
    serde_json::to_string(&serde_json::json!({
        "version": "2.0.0",
        "types": [],
        "queries": [],
        "mutations": [],
        "security": { "tenancy": { "mode": mode, "tenant_claim": "tenant_id" } },
    }))
    .unwrap()
}

fn pool_config(url: &str) -> TenantPoolConfig {
    TenantPoolConfig {
        connection_string:    url.to_string(),
        max_connections:      POOL_MAX,
        connect_timeout_secs: 10,
        idle_timeout_secs:    300,
        // Deliberately left unset: `create_tenant_executor` derives it from the
        // compiled schema's tenancy mode. A test that pre-set it would prove
        // nothing about the registration path.
        search_path:          None,
    }
}

/// Tag this pool's backends so a test can terminate exactly them and nothing else on
/// a shared database. Doubles as a guard that the isolation mechanism does not
/// clobber an operator-supplied connection-string parameter.
fn pool_config_tagged(url: &str, app_name: &str) -> TenantPoolConfig {
    let sep = if url.contains('?') { '&' } else { '?' };
    TenantPoolConfig {
        connection_string: format!("{url}{sep}application_name={app_name}"),
        ..pool_config(url)
    }
}

/// Build the fixture: a `public` decoy plus one relation per tenant schema, each
/// carrying a marker that names where it came from.
async fn setup() -> Option<(String, PostgresAdapter)> {
    let url = try_database_url()?;
    let admin = PostgresAdapter::new(&url).await.expect("connect to the test database");

    for key in [TENANT_A, TENANT_B] {
        exec(&admin, &format!("DROP SCHEMA IF EXISTS tenant_{key} CASCADE")).await;
    }
    exec(&admin, &format!("DROP TABLE IF EXISTS public.{PROBE_RELATION}")).await;

    // The decoy: same unqualified name, wrong rows, no error.
    exec(&admin, &format!("CREATE TABLE public.{PROBE_RELATION} (id int, data jsonb)")).await;
    exec(
        &admin,
        &format!(
            "INSERT INTO public.{PROBE_RELATION} VALUES (0, '{{\"marker\":\"{PUBLIC_DECOY_MARKER}\"}}')"
        ),
    )
    .await;

    for (i, key) in [TENANT_A, TENANT_B].iter().enumerate() {
        exec(&admin, &format!("CREATE SCHEMA tenant_{key}")).await;
        exec(
            &admin,
            &format!("CREATE TABLE tenant_{key}.{PROBE_RELATION} (id int, data jsonb)"),
        )
        .await;
        exec(
            &admin,
            &format!(
                "INSERT INTO tenant_{key}.{PROBE_RELATION} VALUES ({}, '{{\"marker\":\"{key}\"}}')",
                i + 1
            ),
        )
        .await;
    }

    Some((url, admin))
}

async fn teardown(admin: &PostgresAdapter) {
    for key in [TENANT_A, TENANT_B] {
        exec(admin, &format!("DROP SCHEMA IF EXISTS tenant_{key} CASCADE")).await;
    }
    exec(admin, &format!("DROP TABLE IF EXISTS public.{PROBE_RELATION}")).await;
}

async fn exec(adapter: &PostgresAdapter, sql: &str) {
    let _: Vec<HashMap<String, Value>> = adapter
        .execute_raw_query(sql)
        .await
        .unwrap_or_else(|e| panic!("fixture SQL `{sql}`: {e}"));
}

/// What one concurrent read observed.
#[derive(Debug)]
struct Observation {
    /// The PostgreSQL backend that served it — identifies the connection.
    pid:    String,
    /// Schemas the query actually resolved against.
    path:   String,
    /// Marker read from the unqualified probe relation.
    marker: String,
}

/// One probe: holds its connection for `pg_sleep` seconds so several run at once,
/// and reports the backend that served it, which schemas resolved the unqualified
/// relation, and the marker it read from there.
///
/// `path` is reported as well as `marker` because the two failure modes are
/// different: a wrong marker is the silent-wrong-data case, and a missing relation
/// (which surfaces as an `Err`) is the hard-failure case on a deployment where
/// `public` holds no shadowing relation.
async fn probe<A>(executor: &Arc<Executor<A>>) -> Result<Observation, String>
where
    A: fraiseql_core::db::traits::DatabaseAdapter,
{
    let sql = format!(
        "SELECT pg_sleep(0.35) IS NULL AS held, \
         pg_backend_pid()::text AS pid, \
         current_schemas(false)::text AS search_path, \
         (SELECT data->>'marker' FROM {PROBE_RELATION} LIMIT 1) AS marker"
    );
    let rows: Vec<HashMap<String, Value>> =
        executor.adapter().execute_raw_query(&sql).await.map_err(|e| e.to_string())?;
    let row = rows.first().ok_or_else(|| "no row returned".to_string())?;
    let cell = |name: &str| {
        row.get(name).and_then(Value::as_str).map(str::to_owned).ok_or_else(|| {
            format!("column `{name}` was NULL or absent (a relation resolved elsewhere)")
        })
    };
    Ok(Observation {
        pid:    cell("pid")?,
        path:   cell("search_path")?,
        marker: cell("marker")?,
    })
}

/// Drive `CONCURRENCY` probes at once, so the pool must open more than one connection.
async fn concurrent_wave<A>(executor: &Arc<Executor<A>>) -> Vec<Result<Observation, String>>
where
    A: fraiseql_core::db::traits::DatabaseAdapter,
{
    futures::future::join_all((0..CONCURRENCY).map(|_| probe(executor))).await
}

/// #809 core: **every** connection in a tenant's pool must carry the tenant search
/// path, so `CONCURRENCY` simultaneous reads all see the tenant's own rows.
///
/// Pre-fix this fails with `CONCURRENCY - 1` results reading `PUBLIC-DECOY`: only the
/// connection that `configure_search_path` happened to borrow was ever configured.
#[tokio::test]
async fn every_pooled_connection_carries_the_tenant_search_path() {
    let Some((url, admin)) = setup().await else {
        eprintln!("SKIP: DATABASE_URL not set");
        return;
    };

    let executor = create_tenant_executor::<PostgresAdapter>(
        TENANT_A,
        &schema_json_for("schema"),
        &pool_config(&url),
    )
    .await
    .expect("tenant registration");

    let results = concurrent_wave(&executor).await;
    teardown(&admin).await;

    let mut leaked = Vec::new();
    let mut backends = std::collections::HashSet::new();
    for (i, r) in results.iter().enumerate() {
        match r {
            Ok(o) if o.marker == TENANT_A => {
                backends.insert(o.pid.clone());
                assert!(
                    o.path.contains(&format!("tenant_{TENANT_A}")),
                    "probe {i} read the right marker but from {}",
                    o.path
                );
            },
            Ok(o) => leaked.push(format!("probe {i}: marker={} path={}", o.marker, o.path)),
            Err(e) => leaked.push(format!("probe {i}: {e}")),
        }
    }

    assert!(
        leaked.is_empty(),
        "#809: {} of {CONCURRENCY} concurrent tenant reads did not resolve against \
         tenant_{TENANT_A}:\n  {}",
        leaked.len(),
        leaked.join("\n  ")
    );
    // Guard the guard: if the pool only ever used one backend, this test would pass
    // against the broken single-connection `SET` too.
    assert!(
        backends.len() > 1,
        "the pool served all {CONCURRENCY} probes from one backend ({backends:?}); \
         this test proves nothing unless several connections are in play"
    );
}

/// Two tenants driven concurrently against the same database must never observe each
/// other's rows or the shared `public` decoy.
#[tokio::test]
async fn two_tenants_driven_concurrently_never_cross() {
    let Some((url, admin)) = setup().await else {
        eprintln!("SKIP: DATABASE_URL not set");
        return;
    };

    let schema = schema_json_for("schema");
    let exec_a = create_tenant_executor::<PostgresAdapter>(TENANT_A, &schema, &pool_config(&url))
        .await
        .expect("tenant A registration");
    let exec_b = create_tenant_executor::<PostgresAdapter>(TENANT_B, &schema, &pool_config(&url))
        .await
        .expect("tenant B registration");

    let (a_results, b_results) = tokio::join!(concurrent_wave(&exec_a), concurrent_wave(&exec_b));
    teardown(&admin).await;

    for (label, expected, results) in [("A", TENANT_A, &a_results), ("B", TENANT_B, &b_results)] {
        for (i, r) in results.iter().enumerate() {
            let o = r
                .as_ref()
                .unwrap_or_else(|e| panic!("#809: tenant {label} probe {i} failed: {e}"));
            assert_eq!(
                o.marker, expected,
                "#809: tenant {label} probe {i} read `{}` (search_path = {}); expected only \
                 `{expected}`",
                o.marker, o.path
            );
        }
    }
}

/// The isolation must survive the pool losing and re-opening a connection. deadpool
/// creates replacements with no knowledge of anything a previous session was told,
/// so an isolation mechanism that lives in session state degrades to *zero* correct
/// connections after a backend restart or an idle disconnect.
#[tokio::test]
async fn isolation_survives_connection_replacement() {
    let Some((url, admin)) = setup().await else {
        eprintln!("SKIP: DATABASE_URL not set");
        return;
    };

    let executor = create_tenant_executor::<PostgresAdapter>(
        TENANT_A,
        &schema_json_for("schema"),
        &pool_config_tagged(&url, APP_NAME),
    )
    .await
    .expect("tenant registration");

    // Fill the pool and record which backends served it.
    let before: std::collections::HashSet<String> = concurrent_wave(&executor)
        .await
        .into_iter()
        .filter_map(|r| r.ok().map(|o| o.pid))
        .collect();
    assert!(!before.is_empty(), "the warm-up wave produced no observations");

    // Terminate exactly this pool's backends, from an independent connection. Scoped
    // by `application_name` so a shared test database keeps its other connections.
    let _: Vec<HashMap<String, Value>> = admin
        .execute_raw_query(&format!(
            "SELECT pg_terminate_backend(pid) FROM pg_stat_activity \
             WHERE datname = current_database() AND application_name = '{APP_NAME}'"
        ))
        .await
        .expect("terminate backends");

    // The first wave after termination is expected to lose the queries that were
    // already dispatched onto the killed backends; it is what evicts them from the
    // pool. The claim under test is about the *replacements*, so it is the wave
    // after that which must be perfect.
    let _ = concurrent_wave(&executor).await;
    let results = concurrent_wave(&executor).await;
    teardown(&admin).await;

    let after: std::collections::HashSet<String> =
        results.iter().filter_map(|r| r.as_ref().ok().map(|o| o.pid.clone())).collect();
    let ok = results.iter().filter(|r| matches!(r, Ok(o) if o.marker == TENANT_A)).count();
    assert_eq!(
        ok, CONCURRENCY,
        "#809: after connection replacement only {ok}/{CONCURRENCY} reads resolved against \
         tenant_{TENANT_A}: {results:?}"
    );
    // Guard the guard: prove these really are new connections. If the pool had
    // handed back the original backends, this test would say nothing about
    // replacements — which is precisely where the session-`SET` mechanism failed
    // hardest (it degrades to zero correct connections).
    assert!(
        after.is_disjoint(&before),
        "expected every read to be served by a replacement backend; before={before:?} \
         after={after:?}"
    );
}

/// A tenant whose schema holds a relation `public` does **not** shadow must not fail
/// intermittently. This is the loud half of #809's failure scenario (Case B in the
/// issue): without a shared decoy, a connection on the default path errors with
/// `relation ... does not exist`, and which requests hit it is pure luck.
#[tokio::test]
async fn tenant_only_relation_resolves_on_every_connection() {
    let Some((url, admin)) = setup().await else {
        eprintln!("SKIP: DATABASE_URL not set");
        return;
    };
    exec(&admin, &format!("CREATE TABLE tenant_{TENANT_A}.v_iso_only (id int)")).await;

    let executor = create_tenant_executor::<PostgresAdapter>(
        TENANT_A,
        &schema_json_for("schema"),
        &pool_config(&url),
    )
    .await
    .expect("tenant registration");

    let results = futures::future::join_all((0..CONCURRENCY).map(|_| async {
        executor
            .adapter()
            .execute_raw_query(
                "SELECT pg_sleep(0.35) IS NULL AS held, count(*)::text AS n FROM v_iso_only",
            )
            .await
            .map_err(|e| e.to_string())
    }))
    .await;
    teardown(&admin).await;

    let failures: Vec<&String> = results.iter().filter_map(|r| r.as_ref().err()).collect();
    assert!(
        failures.is_empty(),
        "#809: {} of {CONCURRENCY} concurrent reads of a tenant-only relation failed: {failures:?}",
        failures.len()
    );
}

/// Counterweight: a schema that does **not** declare schema-mode tenancy must not
/// acquire a tenant search path. Without this, a fix that unconditionally prefixed
/// every pool with `tenant_{key}` would pass every assertion above while silently
/// changing relation resolution for single-tenant deployments.
#[tokio::test]
async fn non_schema_mode_tenants_keep_the_default_search_path() {
    let Some((url, admin)) = setup().await else {
        eprintln!("SKIP: DATABASE_URL not set");
        return;
    };

    let executor = create_tenant_executor::<PostgresAdapter>(
        TENANT_A,
        &schema_json_for("row"),
        &pool_config(&url),
    )
    .await
    .expect("tenant registration");

    let result = probe(&executor).await;
    teardown(&admin).await;

    let o = result.expect("row-mode read");
    assert!(
        !o.path.contains(&format!("tenant_{TENANT_A}")),
        "row-mode tenancy must not install a schema search path, got {}",
        o.path
    );
    assert_eq!(
        o.marker, PUBLIC_DECOY_MARKER,
        "row-mode tenancy resolves against the default search path"
    );
}
