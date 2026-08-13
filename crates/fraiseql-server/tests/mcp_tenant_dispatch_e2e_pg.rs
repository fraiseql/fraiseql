//! #858 regression: MCP tool calls must go through per-tenant executor dispatch,
//! and the suspended-tenant gate must apply to them.
//!
//! `FraiseQLMcpService` captured `executor_swap.load_full()` — the **default**
//! executor — at session construction and called it directly. It never resolved a
//! tenant key and never consulted `TenantExecutorRegistry`, so an authenticated
//! caller read the boot database rather than their own tenant's, and a tenant
//! suspended through `POST /api/v1/admin/tenants/{key}/suspend` kept reading over
//! MCP while `/graphql` correctly answered 503.
//!
//! **A wrong-database read is silent, which is why this shipped.** The fixture is
//! built to make it silent here too: `public` holds a decoy `v_widget` with its own
//! marker, so a call that lands on the default executor returns rows rather than an
//! error — exactly as it would in production, where the boot database is the
//! control-plane database and has the same relations.
//!
//! Self-skips when no `DATABASE_URL` is set (no `#[ignore]`), so it is inert in the
//! database-free `test` leg and runs in the Dagger `integration: server` suite.
//!
//! **Execution engine:** `PostgreSQL` · **Infrastructure:** `DATABASE_URL` ·
//! **Parallelism:** creates and drops its own `tenant_p09mcp*` schemas and a
//! `public.v_widget` decoy → run `--test-threads=1`.
#![cfg(feature = "mcp")]
#![allow(clippy::unwrap_used, clippy::panic, clippy::print_stderr)] // Reason: test code — panics and skip diagnostics are acceptable

use std::{collections::HashMap, sync::Arc};

use axum::http::HeaderMap;
use fraiseql_core::{
    db::postgres::{PostgresAdapter, PostgresTlsConfig, ReadReplicaPolicy},
    prelude::DatabaseAdapter as _,
    runtime::Executor,
    schema::{CompiledSchema, McpConfig},
};
use fraiseql_server::{
    mcp::handler::FraiseQLMcpService,
    routes::graphql::{AppState, tenant_registry::TenantExecutorRegistry},
    tenancy::{TenantPoolConfig, create_tenant_executor},
};
use fraiseql_test_support::try_database_url;
use serde_json::Value;

/// Tenant keys. `tenant_` is prepended by the schema-isolation module.
const TENANT_A: &str = "p09mcpa";
const TENANT_B: &str = "p09mcpb";
/// A key that is never registered — dispatch must refuse it, not fall back.
const TENANT_UNREGISTERED: &str = "p09mcpz";

/// The relation each tenant schema and `public` all define, so a read that lands on
/// the wrong database succeeds with the wrong rows instead of erroring.
const VIEW: &str = "v_widget";
/// Marker written into the `public` decoy — what the default executor returns.
const DEFAULT_MARKER: &str = "DEFAULT-DATABASE";

/// The marker in a tenant's own row.
///
/// Deliberately not the bare tenant key: a refusal message names the tenant too,
/// so an assertion on the key alone could not tell "served this tenant's rows"
/// apart from "refused this tenant by name".
fn marker(key: &str) -> String {
    format!("ROWS-OF-{key}")
}

/// The compiled schema, in schema-per-tenant mode with one MCP-exposed query.
fn schema_json() -> String {
    serde_json::to_string(&serde_json::json!({
        "version": "2.0.0",
        "types": [{
            "name": "Widget",
            "sql_source": VIEW,
            "fields": [
                { "name": "id", "field_type": "Int" },
                { "name": "marker", "field_type": "String" },
            ],
        }],
        "queries": [{
            "name": "widgets",
            "return_type": "Widget",
            "returns_list": true,
            "sql_source": VIEW,
        }],
        "mutations": [],
        "security": { "tenancy": { "mode": "schema", "tenant_claim": "tenant_id" } },
    }))
    .unwrap()
}

fn compiled_schema() -> CompiledSchema {
    CompiledSchema::from_json(&schema_json(), false).expect("fixture schema must compile")
}

fn pool_config(url: &str) -> TenantPoolConfig {
    TenantPoolConfig {
        connection_string:    url.to_string(),
        max_connections:      2,
        connect_timeout_secs: 10,
        idle_timeout_secs:    300,
        // Left unset on purpose: `create_tenant_executor` derives it from the
        // compiled schema's tenancy mode, which is the registration path the
        // binary uses.
        search_path:          None,
        tls:                  PostgresTlsConfig::default(),
        // Stamped by `make_executor_factory` in the binary; primary-only here.
        read_replica_urls:    Vec::new(),
        read_replica_policy:  ReadReplicaPolicy::default(),
    }
}

/// `[mcp]` with authentication off, so the tenant key comes from `X-Tenant-ID` and
/// this suite tests dispatch rather than token validation.
fn mcp_config() -> McpConfig {
    McpConfig {
        enabled: true,
        require_auth: false,
        ..McpConfig::default()
    }
}

async fn exec(adapter: &PostgresAdapter, sql: &str) {
    let _: Vec<HashMap<String, Value>> = adapter
        .execute_raw_query(sql)
        .await
        .unwrap_or_else(|e| panic!("fixture SQL `{sql}`: {e}"));
}

/// Build the fixture and the `AppState` the binary builds for multi-tenant mode:
/// a real registry holding a real per-tenant PostgreSQL executor for each key.
async fn setup() -> Option<(PostgresAdapter, AppState<PostgresAdapter>)> {
    let url = try_database_url()?;
    let admin = PostgresAdapter::new(&url).await.expect("connect to the test database");

    teardown(&admin).await;

    // The decoy in `public`: what the boot/default executor serves.
    exec(&admin, &format!("CREATE TABLE public.{VIEW} (id int, data jsonb)")).await;
    exec(
        &admin,
        &format!(
            "INSERT INTO public.{VIEW} VALUES (0, '{{\"id\":0,\"marker\":\"{DEFAULT_MARKER}\"}}')"
        ),
    )
    .await;

    for (i, key) in [TENANT_A, TENANT_B].iter().enumerate() {
        exec(&admin, &format!("CREATE SCHEMA tenant_{key}")).await;
        exec(&admin, &format!("CREATE TABLE tenant_{key}.{VIEW} (id int, data jsonb)")).await;
        exec(
            &admin,
            &format!(
                "INSERT INTO tenant_{key}.{VIEW} VALUES ({}, '{{\"id\":{},\"marker\":\"{}\"}}')",
                i + 1,
                i + 1,
                marker(key)
            ),
        )
        .await;
    }

    let default_executor = Arc::new(Executor::new(compiled_schema(), Arc::new(admin.clone())));
    let registry =
        Arc::new(TenantExecutorRegistry::new(Arc::new(arc_swap::ArcSwap::from(default_executor))));

    for key in [TENANT_A, TENANT_B] {
        let executor =
            create_tenant_executor::<PostgresAdapter>(key, &schema_json(), &pool_config(&url))
                .await
                .unwrap_or_else(|e| panic!("provision tenant {key}: {e}"));
        registry.upsert(key, executor);
    }

    let state = AppState::new(Arc::new(Executor::new(compiled_schema(), Arc::new(admin.clone()))))
        .with_tenant_registry(registry);

    Some((admin, state))
}

async fn teardown(admin: &PostgresAdapter) {
    for key in [TENANT_A, TENANT_B] {
        exec(admin, &format!("DROP SCHEMA IF EXISTS tenant_{key} CASCADE")).await;
    }
    exec(admin, &format!("DROP TABLE IF EXISTS public.{VIEW}")).await;
}

fn headers_for(tenant: Option<&str>) -> HeaderMap {
    let mut headers = HeaderMap::new();
    if let Some(key) = tenant {
        headers.insert("X-Tenant-ID", key.parse().expect("valid header value"));
    }
    headers
}

/// Call the `widgets` MCP tool as `tenant`, returning the tool result's text.
async fn call_as(state: &AppState<PostgresAdapter>, tenant: Option<&str>) -> (bool, String) {
    let service = FraiseQLMcpService::new(state.clone(), mcp_config());
    let result = service
        .call_tool_authenticated(
            "widgets",
            None,
            None,
            format!("mcp-p09-{}", tenant.unwrap_or("default")),
            &headers_for(tenant),
        )
        .await;
    let text = result
        .content
        .first()
        .and_then(|c| c.as_text().map(|t| t.text.clone()))
        .unwrap_or_default();
    (result.is_error == Some(true), text)
}

/// #858 core: an MCP tool call must run on the caller's tenant executor.
///
/// Pre-fix both calls return `DEFAULT-DATABASE`: the session captured the default
/// executor and no tenant key was ever resolved.
#[tokio::test]
async fn an_mcp_tool_call_runs_on_the_callers_tenant_executor() {
    let Some((admin, state)) = setup().await else {
        eprintln!("SKIP: DATABASE_URL not set");
        return;
    };

    for key in [TENANT_A, TENANT_B] {
        let (is_error, text) = call_as(&state, Some(key)).await;
        assert!(!is_error, "tenant {key}: MCP call failed: {text}");
        assert!(
            text.contains(&marker(key)),
            "#858: MCP call as tenant {key} did not read that tenant's data: {text}",
        );
        assert!(
            !text.contains(DEFAULT_MARKER),
            "#858: MCP call as tenant {key} read the DEFAULT executor's database: {text}",
        );
    }

    // Control: with no tenant key the default executor is correct, so the
    // assertions above are about dispatch and not about the fixture.
    let (is_error, text) = call_as(&state, None).await;
    assert!(!is_error, "unkeyed MCP call failed: {text}");
    assert!(
        text.contains(DEFAULT_MARKER),
        "an unkeyed call must still use the default executor: {text}",
    );

    teardown(&admin).await;
}

/// #858: a suspended tenant is refused over MCP exactly as over `/graphql`, and
/// resuming restores service.
#[tokio::test]
async fn a_suspended_tenant_is_refused_over_mcp() {
    let Some((admin, state)) = setup().await else {
        eprintln!("SKIP: DATABASE_URL not set");
        return;
    };

    let registry = state.tenant_registry().expect("registry wired").clone();

    // Precondition: the tenant works before suspension.
    let (is_error, text) = call_as(&state, Some(TENANT_B)).await;
    assert!(!is_error, "precondition: tenant {TENANT_B} must work before suspension: {text}");

    registry.suspend(TENANT_B).expect("suspend the tenant");

    let (is_error, text) = call_as(&state, Some(TENANT_B)).await;
    assert!(
        is_error,
        "#858: a suspended tenant kept reading over MCP while /graphql answered 503: {text}",
    );
    assert!(
        !text.contains(&marker(TENANT_B)) && !text.contains(DEFAULT_MARKER),
        "a refused call must return no rows at all: {text}",
    );

    // The other tenant is unaffected — suspension is per-tenant, not global.
    let (is_error, other) = call_as(&state, Some(TENANT_A)).await;
    assert!(
        !is_error,
        "tenant {TENANT_A} must be unaffected by {TENANT_B}'s suspension: {other}"
    );
    assert!(other.contains(&marker(TENANT_A)), "{other}");

    registry.resume(TENANT_B).expect("resume the tenant");
    let (is_error, text) = call_as(&state, Some(TENANT_B)).await;
    assert!(!is_error, "resume must restore MCP service: {text}");
    assert!(text.contains(&marker(TENANT_B)), "{text}");

    teardown(&admin).await;
}

/// #858: an unregistered tenant key is refused, never silently served from the
/// default executor. Falling back would hand one caller another tenant's data.
#[tokio::test]
async fn an_unregistered_tenant_key_is_refused_not_silently_defaulted() {
    let Some((admin, state)) = setup().await else {
        eprintln!("SKIP: DATABASE_URL not set");
        return;
    };

    let (is_error, text) = call_as(&state, Some(TENANT_UNREGISTERED)).await;

    assert!(is_error, "#858: an unregistered tenant key was served: {text}");
    assert!(
        !text.contains(DEFAULT_MARKER),
        "#858: an unregistered tenant key fell back to the default executor: {text}",
    );

    teardown(&admin).await;
}
