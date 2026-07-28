//! #859 regression: `DELETE /api/v1/admin/tenants/{key}` must not report a tenant
//! "removed" while every row it owned survives in PostgreSQL.
//!
//! `delete_tenant_handler` dropped the in-memory registry entry, logged
//! `"tenant executor removed"` and recorded a `Deleted` audit event.
//! `destroy_tenant_schema` — whose own doc comment claimed "Called from the delete
//! tenant handler when `tenancy.mode = \"schema\"`" — had **no callers anywhere in
//! the workspace**. Because registration provisions with `CREATE SCHEMA IF NOT
//! EXISTS`, re-registering the same key silently adopted the surviving schema and
//! served the previous tenant's rows to the next one.
//!
//! Dropping a schema is irreversible, so the fix is not "always drop": the API now
//! takes an explicit `?purge=true`, and the default response says plainly that the
//! data was retained instead of claiming an erasure that did not happen. Both
//! branches are asserted here, along with the ordering property that matters — a
//! purge that fails must leave the tenant registered rather than reporting success.
//!
//! Self-skips when no `DATABASE_URL` is set (no `#[ignore]`), so it is inert in the
//! database-free `test` leg and runs in the Dagger `integration: server` suite.
//!
//! **Execution engine:** `PostgreSQL` · **Infrastructure:** `DATABASE_URL` ·
//! **Parallelism:** creates and drops its own `tenant_p04life*` schemas →
//! run `--test-threads=1`.
#![allow(clippy::unwrap_used, clippy::panic, clippy::print_stderr)] // Reason: test code — panics and skip diagnostics are acceptable

use std::{collections::HashMap, sync::Arc};

use axum::extract::{Path, Query, State};
use fraiseql_core::{
    db::postgres::{PostgresAdapter, PostgresTlsConfig},
    prelude::DatabaseAdapter as _,
    runtime::Executor,
    schema::CompiledSchema,
};
use fraiseql_server::{
    extractors::OptionalSecurityContext,
    routes::{
        api::tenant_admin::{
            DeleteTenantQuery, TenantRegistrationRequest, delete_tenant_handler,
            upsert_tenant_handler,
        },
        graphql::{AppState, tenant_registry::TenantExecutorRegistry},
    },
    tenancy::{TenantPoolConfig, make_executor_factory},
};
use fraiseql_test_support::try_database_url;
use serde_json::{Value, json};

/// Recycled key: the whole point of #859 is what the *second* tenant sees.
const TENANT_KEY: &str = "p04lifea";

const SECRET_ROW: &str = "customer-A-row";

fn tenancy_schema_json() -> Value {
    json!({
        "version": "2.0.0",
        "types": [],
        "queries": [],
        "mutations": [],
        "security": { "tenancy": { "mode": "schema", "tenant_claim": "tenant_id" } },
    })
}

fn registration(url: &str) -> TenantRegistrationRequest {
    TenantRegistrationRequest {
        schema:                     tenancy_schema_json(),
        connection:                 TenantPoolConfig {
            connection_string:    url.to_string(),
            max_connections:      2,
            connect_timeout_secs: 10,
            idle_timeout_secs:    300,
            search_path:          None,
            tls:                  PostgresTlsConfig::default(),
        },
        max_requests_per_sec:       None,
        max_concurrent:             None,
        max_storage_bytes_advisory: None,
        cost_budget:                None,
    }
}

/// An `AppState` wired exactly as the binary wires it for multi-tenant mode: a real
/// registry and the real PostgreSQL executor factory.
async fn setup() -> Option<(String, PostgresAdapter, AppState<PostgresAdapter>)> {
    let url = try_database_url()?;
    let admin = PostgresAdapter::new(&url).await.expect("connect to the test database");
    exec(&admin, &format!("DROP SCHEMA IF EXISTS tenant_{TENANT_KEY} CASCADE")).await;

    let default_executor =
        Arc::new(Executor::new(CompiledSchema::default(), Arc::new(admin.clone())));
    let registry =
        Arc::new(TenantExecutorRegistry::new(Arc::new(arc_swap::ArcSwap::from(default_executor))));

    let state =
        AppState::new(Arc::new(Executor::new(CompiledSchema::default(), Arc::new(admin.clone()))))
            .with_tenant_registry(registry)
            .with_tenant_executor_factory(make_executor_factory::<PostgresAdapter>(
                PostgresTlsConfig::default(),
            ));

    Some((url, admin, state))
}

async fn exec(adapter: &PostgresAdapter, sql: &str) {
    let _: Vec<HashMap<String, Value>> = adapter
        .execute_raw_query(sql)
        .await
        .unwrap_or_else(|e| panic!("fixture SQL `{sql}`: {e}"));
}

async fn schema_exists(adapter: &PostgresAdapter, schema: &str) -> bool {
    let rows: Vec<HashMap<String, Value>> = adapter
        .execute_raw_query(&format!(
            "SELECT count(*)::text AS n FROM pg_namespace WHERE nspname = '{schema}'"
        ))
        .await
        .expect("pg_namespace probe");
    rows.first().and_then(|r| r.get("n")).and_then(Value::as_str) != Some("0")
}

/// Register the tenant and write one row into its schema.
async fn register_with_a_secret(
    state: &AppState<PostgresAdapter>,
    admin: &PostgresAdapter,
    url: &str,
) {
    let _ = upsert_tenant_handler(
        State(state.clone()),
        Path(TENANT_KEY.to_string()),
        OptionalSecurityContext(None),
        axum::Json(registration(url)),
    )
    .await
    .expect("tenant registration");

    exec(admin, &format!("CREATE TABLE tenant_{TENANT_KEY}.tb_secret (v text)")).await;
    exec(
        admin,
        &format!("INSERT INTO tenant_{TENANT_KEY}.tb_secret VALUES ('{SECRET_ROW}')"),
    )
    .await;
}

async fn delete(state: &AppState<PostgresAdapter>, purge: bool) -> String {
    let response = delete_tenant_handler(
        State(state.clone()),
        Path(TENANT_KEY.to_string()),
        Query(DeleteTenantQuery { purge }),
        OptionalSecurityContext(None),
    )
    .await
    .expect("tenant deletion");
    serde_json::to_string(&response.0).expect("serialize response")
}

/// #859 core: `?purge=true` must actually drop the schema, and a tenant registered
/// afterwards under the recycled key must not inherit the previous tenant's rows.
#[tokio::test]
async fn purging_delete_drops_the_schema_so_a_recycled_key_starts_empty() {
    let Some((url, admin, state)) = setup().await else {
        eprintln!("SKIP: DATABASE_URL not set");
        return;
    };
    register_with_a_secret(&state, &admin, &url).await;
    assert!(
        schema_exists(&admin, &format!("tenant_{TENANT_KEY}")).await,
        "fixture precondition"
    );

    let body = delete(&state, true).await;
    assert!(body.contains("purged"), "the response must name what it did: {body}");

    assert!(
        !schema_exists(&admin, &format!("tenant_{TENANT_KEY}")).await,
        "#859: DELETE ?purge=true reported success but tenant_{TENANT_KEY} still exists"
    );

    // Re-register under the recycled key; the new tenant must see nothing.
    register_with_a_secret(&state, &admin, &url).await;
    let rows: Vec<HashMap<String, Value>> = admin
        .execute_raw_query(&format!(
            "SELECT count(*)::text AS n FROM tenant_{TENANT_KEY}.tb_secret"
        ))
        .await
        .expect("count secret rows");
    // One row, because the helper writes one — not two, which is what surviving
    // data would produce.
    assert_eq!(
        rows.first().and_then(|r| r.get("n")).and_then(Value::as_str),
        Some("1"),
        "#859: a tenant registered under a recycled key inherited the previous tenant's rows"
    );

    exec(&admin, &format!("DROP SCHEMA IF EXISTS tenant_{TENANT_KEY} CASCADE")).await;
}

/// The default is non-destructive — and must say so. Reporting `"removed"` while
/// the rows survive is the half of #859 that misleads an operator into believing an
/// offboarding erasure completed.
#[tokio::test]
async fn default_delete_retains_the_schema_and_says_so() {
    let Some((url, admin, state)) = setup().await else {
        eprintln!("SKIP: DATABASE_URL not set");
        return;
    };
    register_with_a_secret(&state, &admin, &url).await;

    let body = delete(&state, false).await;

    assert!(
        schema_exists(&admin, &format!("tenant_{TENANT_KEY}")).await,
        "the default must not destroy data"
    );
    assert!(
        body.contains("retained"),
        "#859: the response must state that the tenant's data was retained, got: {body}"
    );
    assert!(
        body.contains(&format!("tenant_{TENANT_KEY}")),
        "#859: the response must name the schema an operator still has to deal with, got: {body}"
    );

    exec(&admin, &format!("DROP SCHEMA IF EXISTS tenant_{TENANT_KEY} CASCADE")).await;
}

/// The registry entry is the only handle on the adapter that can run the DDL, so a
/// purge must resolve it *before* removing the tenant and must not remove the tenant
/// if the drop fails. Otherwise a failed purge leaves data behind with no way to
/// retry through the API — while having already answered "removed".
///
/// The failure is induced with a real lock conflict rather than a mock: another
/// session holds `ACCESS SHARE` on a table inside the schema for several seconds,
/// and the tenant's own pool carries `lock_timeout` so `DROP SCHEMA … CASCADE`
/// gives up rather than waiting. That `lock_timeout` arrives through the
/// connection string's `options` parameter, which doubles as a live check that
/// composing the tenant search path into the startup options does not clobber an
/// operator's own settings (#809).
#[tokio::test]
async fn a_failed_purge_leaves_the_tenant_registered() {
    let Some((url, admin, state)) = setup().await else {
        eprintln!("SKIP: DATABASE_URL not set");
        return;
    };

    let sep = if url.contains('?') { '&' } else { '?' };
    let timeout_url = format!("{url}{sep}options=-c%20lock_timeout%3D400");
    let mut request = registration(&timeout_url);
    request.connection.connection_string.clone_from(&timeout_url);
    let _ = upsert_tenant_handler(
        State(state.clone()),
        Path(TENANT_KEY.to_string()),
        OptionalSecurityContext(None),
        axum::Json(request),
    )
    .await
    .expect("tenant registration");
    exec(&admin, &format!("CREATE TABLE tenant_{TENANT_KEY}.tb_secret (v text)")).await;
    exec(
        &admin,
        &format!("INSERT INTO tenant_{TENANT_KEY}.tb_secret VALUES ('{SECRET_ROW}')"),
    )
    .await;

    // A plain SELECT holds ACCESS SHARE for the duration of the statement, which is
    // enough to block the ACCESS EXCLUSIVE that DROP SCHEMA CASCADE needs.
    let blocker = PostgresAdapter::new(&url).await.expect("blocker connection");
    let blocker_handle = tokio::spawn(async move {
        let _: Vec<HashMap<String, Value>> = blocker
            .execute_raw_query(&format!("SELECT pg_sleep(3) FROM tenant_{TENANT_KEY}.tb_secret"))
            .await
            .unwrap_or_default();
    });
    tokio::time::sleep(std::time::Duration::from_millis(500)).await;

    let result = delete_tenant_handler(
        State(state.clone()),
        Path(TENANT_KEY.to_string()),
        Query(DeleteTenantQuery { purge: true }),
        OptionalSecurityContext(None),
    )
    .await;

    assert!(result.is_err(), "#859: a purge whose DDL failed must not report success");
    assert!(
        state
            .tenant_registry()
            .expect("registry")
            .tenant_keys()
            .contains(&TENANT_KEY.to_string()),
        "#859: a failed purge must leave the tenant registered so it can be retried"
    );
    assert!(
        schema_exists(&admin, &format!("tenant_{TENANT_KEY}")).await,
        "the schema must still be there after a failed purge"
    );

    // Wait the blocker out rather than aborting: aborting the task does not cancel
    // the statement already running on the server, so the lock would still be held.
    blocker_handle.await.expect("blocker task");

    // And the retry must now succeed through the same endpoint.
    let body = delete(&state, true).await;
    assert!(body.contains("purged"), "retry after the lock cleared: {body}");
    assert!(!schema_exists(&admin, &format!("tenant_{TENANT_KEY}")).await);
}
