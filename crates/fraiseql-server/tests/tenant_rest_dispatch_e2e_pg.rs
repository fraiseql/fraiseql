//! #1550: with the tenancy runtime on, a REST request runs on the caller's tenant executor.
//!
//! REST used to build every handler from the default executor, snapshotted when the router
//! was built, so a tenant's REST request was answered from the default `search_path` and its
//! write landed there. GraphQL dispatched the same request to the tenant. The fixture makes
//! the defect silent rather than loud: `public` carries a decoy of every relation and
//! function the tenant has, so the wrong executor answers with no error at all.
//!
//! **Execution engine:** `PostgreSQL` · **Infrastructure:** `DATABASE_URL` ·
//! **Parallelism:** creates and drops `tenant_p1550rest` and `public` decoys named
//! `*_p1550_*` → run `--test-threads=1`.
#![cfg(feature = "rest")]
#![allow(clippy::unwrap_used, clippy::panic, clippy::print_stderr)] // Reason: test code

use std::{collections::HashMap, sync::Arc};

use axum::{
    body::Body,
    extract::{Path, State},
    http::{Request, StatusCode},
};
use fraiseql_cli::schema::{ConvertOptions, SchemaConverter, intermediate::IntermediateSchema};
use fraiseql_core::{
    db::postgres::{PostgresAdapter, PostgresTlsConfig, ReadReplicaPolicy},
    prelude::DatabaseAdapter as _,
    runtime::Executor,
    schema::CompiledSchema,
};
use fraiseql_server::{
    extractors::OptionalSecurityContext,
    routes::{
        api::tenant_admin::{TenantRegistrationRequest, upsert_tenant_handler},
        graphql::{AppState, tenant_registry::TenantExecutorRegistry},
        rest::{RestMountConfig, rest_router},
    },
    tenancy::{TenantPoolConfig, make_executor_factory},
};
use fraiseql_test_support::try_database_url;
use serde_json::{Value, json};
use tower::ServiceExt as _;

const TENANT: &str = "p1550rest";
const TENANT_ROW: &str = "TENANT-ROW";
const PUBLIC_DECOY: &str = "PUBLIC-DECOY";

/// The schema both the default executor and the tenant serve: one REST resource over
/// `v_p1550_probe`, written through `fn_p1550_create`.
fn schema() -> CompiledSchema {
    let intermediate: IntermediateSchema = serde_json::from_value(json!({
        "types": [{
            "name": "Probe", "sql_source": "v_p1550_probe", "is_input": false,
            "fields": [{ "name": "id", "type": "ID", "nullable": false },
                       { "name": "label", "type": "String", "nullable": false }]
        }],
        "queries": [{
            "name": "probes", "return_type": "Probe", "returns_list": true, "nullable": false,
            "sql_source": "v_p1550_probe", "arguments": [], "rest_stream": true
        }],
        "mutations": [{
            "name": "createProbe", "return_type": "Probe", "operation": "insert",
            "sql_source": "fn_p1550_create",
            "arguments": [{ "name": "label", "type": "String", "nullable": false }]
        }, {
            "name": "updateProbe", "return_type": "Probe", "operation": "update",
            "sql_source": "fn_p1550_update",
            "arguments": [{ "name": "id", "type": "ID", "nullable": false },
                          { "name": "label", "type": "String", "nullable": true }]
        }, {
            "name": "deleteProbe", "return_type": "Probe", "operation": "delete",
            "sql_source": "fn_p1550_delete",
            "arguments": [{ "name": "id", "type": "ID", "nullable": false }]
        }],
        "security": { "tenancy": { "mode": "schema", "tenant_claim": "tenant_id" } }
    }))
    .unwrap();
    let mut schema = SchemaConverter::convert_artifact(intermediate, &ConvertOptions::default())
        .unwrap()
        .schema;
    schema.rest_config = Some(fraiseql_core::schema::RestConfig {
        enabled: true,
        ..fraiseql_core::schema::RestConfig::default()
    });
    schema.build_indexes();
    schema
}

/// The 13 columns of `mutation_response`, the row every write function returns.
const RESPONSE_ROW: &str = "succeeded boolean, state_changed boolean, error_class text, \
     status_detail text, http_status smallint, message text, entity_id uuid, entity_type \
     text, entity jsonb, updated_fields text[], cascade jsonb, error_detail jsonb, metadata \
     jsonb";

/// The relations and functions of one schema, each row naming the schema it lives in.
fn objects(schema: &str, marker: &str) -> Vec<String> {
    vec![
        format!("CREATE TABLE {schema}.tb_p1550_probe (id uuid PRIMARY KEY, label text)"),
        format!("INSERT INTO {schema}.tb_p1550_probe VALUES (gen_random_uuid(), '{marker}')"),
        format!(
            "CREATE VIEW {schema}.v_p1550_probe AS SELECT id, jsonb_build_object('id', id, \
             'label', label) AS data FROM {schema}.tb_p1550_probe"
        ),
        format!(
            "CREATE FUNCTION {schema}.fn_p1550_create(p_label text) RETURNS TABLE(succeeded \
             boolean, state_changed boolean, error_class text, status_detail text, http_status \
             smallint, message text, entity_id uuid, entity_type text, entity jsonb, \
             updated_fields text[], cascade jsonb, error_detail jsonb, metadata jsonb) \
             LANGUAGE plpgsql AS $$ DECLARE v uuid := gen_random_uuid(); BEGIN INSERT INTO \
             {schema}.tb_p1550_probe VALUES (v, p_label); RETURN QUERY SELECT * FROM \
             fraiseql.mutation_ok(jsonb_build_object('id', v, 'label', p_label), v, 'Probe'); \
             END $$"
        ),
        format!(
            "CREATE FUNCTION {schema}.fn_p1550_update(p_id text, p_label text) RETURNS \
             TABLE({RESPONSE_ROW}) LANGUAGE plpgsql AS $$ BEGIN UPDATE {schema}.tb_p1550_probe \
             SET label = p_label WHERE id = p_id::uuid; RETURN QUERY SELECT * FROM \
             fraiseql.mutation_ok(jsonb_build_object('id', p_id, 'label', p_label), \
             p_id::uuid, 'Probe'); END $$"
        ),
        format!(
            "CREATE FUNCTION {schema}.fn_p1550_delete(p_id text) RETURNS TABLE({RESPONSE_ROW}) \
             LANGUAGE plpgsql AS $$ BEGIN DELETE FROM {schema}.tb_p1550_probe WHERE id = \
             p_id::uuid; RETURN QUERY SELECT * FROM fraiseql.mutation_ok(jsonb_build_object('id', \
             p_id), p_id::uuid, 'Probe'); END $$"
        ),
    ]
}

async fn exec(adapter: &PostgresAdapter, sql: &str) {
    let _: Vec<HashMap<String, Value>> = adapter
        .execute_raw_query(sql)
        .await
        .unwrap_or_else(|e| panic!("fixture SQL `{sql}`: {e}"));
}

async fn tenant_row_id(adapter: &PostgresAdapter) -> String {
    let rows: Vec<HashMap<String, Value>> = adapter
        .execute_raw_query(&format!(
            "SELECT id::text AS id FROM tenant_{TENANT}.tb_p1550_probe WHERE label = '{TENANT_ROW}'"
        ))
        .await
        .unwrap();
    rows[0]["id"].as_str().unwrap().to_string()
}

async fn labels_in(adapter: &PostgresAdapter, schema: &str) -> Vec<String> {
    let rows: Vec<HashMap<String, Value>> = adapter
        .execute_raw_query(&format!("SELECT label FROM {schema}.tb_p1550_probe ORDER BY label"))
        .await
        .unwrap();
    rows.iter().map(|r| r["label"].as_str().unwrap().to_string()).collect()
}

fn registration(url: &str) -> TenantRegistrationRequest {
    TenantRegistrationRequest {
        cost_budget_per_actor:      HashMap::new(),
        schema:                     serde_json::to_value(schema()).unwrap(),
        connection:                 TenantPoolConfig {
            connection_string:    url.to_string(),
            max_connections:      2,
            connect_timeout_secs: 10,
            idle_timeout_secs:    300,
            search_path:          None,
            tls:                  PostgresTlsConfig::default(),
            read_replica_urls:    Vec::new(),
            read_replica_policy:  ReadReplicaPolicy::default(),
            vector_scan:          fraiseql_core::db::postgres::VectorScanConfig::default(),
        },
        max_requests_per_sec:       None,
        max_concurrent:             None,
        max_storage_bytes_advisory: None,
        cost_budget:                None,
        cost_budget_per_minute:     None,
    }
}

/// An `AppState` wired as the binary wires it for multi-tenant mode, the default executor
/// serving the same schema over `public`, and the tenant registered over `tenant_<key>`.
async fn setup() -> Option<(PostgresAdapter, AppState)> {
    let url = try_database_url()?;
    let admin = PostgresAdapter::new(&url).await.expect("connect to the test database");
    let (client, connection) = tokio_postgres::connect(&url, tokio_postgres::NoTls).await.unwrap();
    tokio::spawn(connection);
    client
        .batch_execute(include_str!("../../fraiseql-cli/sql/helpers/mutation_response.sql"))
        .await
        .unwrap();
    exec(&admin, &format!("DROP SCHEMA IF EXISTS tenant_{TENANT} CASCADE")).await;
    exec(&admin, "DROP VIEW IF EXISTS public.v_p1550_probe").await;
    exec(&admin, "DROP TABLE IF EXISTS public.tb_p1550_probe").await;
    exec(&admin, "DROP FUNCTION IF EXISTS public.fn_p1550_create(text)").await;
    exec(&admin, "DROP FUNCTION IF EXISTS public.fn_p1550_update(text, text)").await;
    exec(&admin, "DROP FUNCTION IF EXISTS public.fn_p1550_delete(text)").await;
    for stmt in objects("public", PUBLIC_DECOY) {
        exec(&admin, &stmt).await;
    }

    let default_executor = Arc::new(Executor::new(schema(), Arc::new(admin.clone())));
    let registry = Arc::new(TenantExecutorRegistry::new(Arc::new(arc_swap::ArcSwap::from(
        Arc::clone(&default_executor),
    ))));
    let state = AppState::new(default_executor)
        .with_tenant_registry(registry)
        .with_tenant_executor_factory(make_executor_factory::<PostgresAdapter>(
            PostgresTlsConfig::default(),
            ReadReplicaPolicy::default(),
            fraiseql_core::db::postgres::VectorScanConfig::default(),
        ));

    let _ = upsert_tenant_handler(
        State(state.clone()),
        Path(TENANT.to_string()),
        OptionalSecurityContext(None),
        axum::Json(registration(&url)),
    )
    .await
    .expect("tenant registration");
    for stmt in objects(&format!("tenant_{TENANT}"), TENANT_ROW) {
        exec(&admin, &stmt).await;
    }
    Some((admin, state))
}

fn router(state: &AppState) -> axum::Router {
    rest_router(
        state,
        &RestMountConfig {
            compression_enabled: false,
            auth_layer_attached: false,
            export:              Arc::default(),
        },
    )
    .expect("the REST router mounts")
}

async fn send(router: axum::Router, request: Request<Body>) -> (StatusCode, Value) {
    let response = router.oneshot(request).await.unwrap();
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    (status, serde_json::from_slice(&bytes).unwrap_or(Value::Null))
}

/// A tenant's REST read is answered from its own schema, not the default one.
#[tokio::test]
async fn a_tenants_rest_read_runs_on_its_own_executor() {
    let Some((_admin, state)) = setup().await else {
        eprintln!("skipping #1550: DATABASE_URL not set");
        return;
    };
    let request = Request::get("/rest/v1/probes")
        .header("x-tenant-id", TENANT)
        .body(Body::empty())
        .unwrap();
    let (status, body) = send(router(&state), request).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let text = body.to_string();
    assert!(text.contains(TENANT_ROW), "the tenant's row: {body}");
    assert!(!text.contains(PUBLIC_DECOY), "never the default schema's: {body}");
}

/// A tenant's REST write runs the tenant's function and lands in the tenant's schema.
#[tokio::test]
async fn a_tenants_rest_write_lands_in_its_own_schema() {
    let Some((admin, state)) = setup().await else {
        return;
    };
    let request = Request::post("/rest/v1/probes")
        .header("x-tenant-id", TENANT)
        .header("content-type", "application/json")
        .body(Body::from(json!({ "label": "WRITTEN" }).to_string()))
        .unwrap();
    let (status, body) = send(router(&state), request).await;
    assert!(status.is_success(), "{status} {body}");
    assert_eq!(
        labels_in(&admin, &format!("tenant_{TENANT}")).await,
        [TENANT_ROW, "WRITTEN"],
        "the write is the tenant's"
    );
    assert_eq!(labels_in(&admin, "public").await, [PUBLIC_DECOY], "public is untouched");
}

/// PUT, PATCH and DELETE on a tenant's row reach the tenant's functions and rows: the
/// default executor would find no such row in `public` and leave the tenant's untouched.
#[tokio::test]
async fn a_tenants_rest_update_and_delete_run_on_its_own_executor() {
    let Some((admin, state)) = setup().await else {
        return;
    };
    let id = tenant_row_id(&admin).await;
    let tenant_schema = format!("tenant_{TENANT}");
    for (method, label) in [("PUT", "PUT-LABEL"), ("PATCH", "PATCH-LABEL")] {
        let request = Request::builder()
            .method(method)
            .uri(format!("/rest/v1/probes/{id}"))
            .header("x-tenant-id", TENANT)
            .header("content-type", "application/json")
            .body(Body::from(json!({ "label": label }).to_string()))
            .unwrap();
        let (status, body) = send(router(&state), request).await;
        assert!(status.is_success(), "{method}: {status} {body}");
        assert_eq!(labels_in(&admin, &tenant_schema).await, [label], "{method}");
    }
    let request = Request::delete(format!("/rest/v1/probes/{id}"))
        .header("x-tenant-id", TENANT)
        .body(Body::empty())
        .unwrap();
    let (status, body) = send(router(&state), request).await;
    assert!(status.is_success(), "DELETE: {status} {body}");
    assert!(labels_in(&admin, &tenant_schema).await.is_empty(), "the tenant's row is gone");
    assert_eq!(labels_in(&admin, "public").await, [PUBLIC_DECOY], "public is untouched");
}

/// The NDJSON read negotiates the same tenant scope as the JSON one.
#[tokio::test]
async fn a_tenants_ndjson_read_runs_on_its_own_executor() {
    let Some((_admin, state)) = setup().await else {
        return;
    };
    let request = Request::get("/rest/v1/probes")
        .header("x-tenant-id", TENANT)
        .header("accept", "application/x-ndjson")
        .body(Body::empty())
        .unwrap();
    let response = router(&state).oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let text = String::from_utf8_lossy(&bytes);
    assert!(text.contains(TENANT_ROW) && !text.contains(PUBLIC_DECOY), "{text}");
}

/// The resource stream resolves the tenant too: a tenant whose schema serves no REST is
/// told so, rather than answered from the default executor's routes.
#[tokio::test]
async fn a_tenants_stream_is_scoped_to_its_own_routes() {
    let Some((_admin, state)) = setup().await else {
        return;
    };
    let url = try_database_url().unwrap();
    let mut without_rest = schema();
    without_rest.rest_config = None;
    let _ = upsert_tenant_handler(
        State(state.clone()),
        Path(TENANT.to_string()),
        OptionalSecurityContext(None),
        axum::Json(TenantRegistrationRequest {
            schema: serde_json::to_value(without_rest).unwrap(),
            ..registration(&url)
        }),
    )
    .await
    .expect("tenant re-registration");
    let request = Request::get("/rest/v1/probes/stream")
        .header("x-tenant-id", TENANT)
        .header("accept", "text/event-stream")
        .body(Body::empty())
        .unwrap();
    let (status, body) = send(router(&state), request).await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{body}");
}

/// A request that names no tenant is still served by the default executor.
#[tokio::test]
async fn a_request_with_no_tenant_is_served_by_the_default_executor() {
    let Some((_admin, state)) = setup().await else {
        return;
    };
    let request = Request::get("/rest/v1/probes").body(Body::empty()).unwrap();
    let (status, body) = send(router(&state), request).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert!(body.to_string().contains(PUBLIC_DECOY), "{body}");
}

/// An unregistered tenant is refused, as GraphQL refuses it: never served the default data.
#[tokio::test]
async fn an_unregistered_tenant_is_refused() {
    let Some((_admin, state)) = setup().await else {
        return;
    };
    let request = Request::get("/rest/v1/probes")
        .header("x-tenant-id", "p1550nobody")
        .body(Body::empty())
        .unwrap();
    let (status, body) = send(router(&state), request).await;
    assert!(!status.is_success(), "{status} {body}");
    assert!(!body.to_string().contains(PUBLIC_DECOY), "{body}");
}

/// A suspended tenant is refused on REST, through the same dispatch `/graphql` uses.
#[tokio::test]
async fn a_suspended_tenant_is_refused() {
    let Some((_admin, state)) = setup().await else {
        return;
    };
    state.tenant_registry().unwrap().suspend(TENANT).unwrap();
    let request = Request::get("/rest/v1/probes")
        .header("x-tenant-id", TENANT)
        .body(Body::empty())
        .unwrap();
    let (status, body) = send(router(&state), request).await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE, "{body}");
}

/// A tenant's routes follow its registration: re-registered with REST disabled, the
/// routes derived from its previous schema are not served.
#[tokio::test]
async fn a_re_registered_tenant_is_served_its_new_routes() {
    let Some((_admin, state)) = setup().await else {
        return;
    };
    let router = router(&state);
    let get = || {
        Request::get("/rest/v1/probes")
            .header("x-tenant-id", TENANT)
            .body(Body::empty())
            .unwrap()
    };
    assert_eq!(send(router.clone(), get()).await.0, StatusCode::OK, "derived and cached");

    let url = try_database_url().unwrap();
    let mut without_rest = schema();
    without_rest.rest_config = None;
    let _ = upsert_tenant_handler(
        State(state.clone()),
        Path(TENANT.to_string()),
        OptionalSecurityContext(None),
        axum::Json(TenantRegistrationRequest {
            schema: serde_json::to_value(without_rest).unwrap(),
            ..registration(&url)
        }),
    )
    .await
    .expect("tenant re-registration");
    let (status, body) = send(router, get()).await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{body}");
}

/// A dispatched function's `fraiseql_query` write runs on its tenant's executor (#1550): the
/// bridge (after:mutation, after:ingest, cron, after:capture) took the default executor
/// whatever tenant its `run_as` identity carried.
#[cfg(feature = "functions-runtime")]
mod function_writes {
    use fraiseql_core::security::SecurityContext;
    use fraiseql_functions::host::live::QueryExecutor as _;
    use fraiseql_server::query_bridge::{ExecutorSource, RunAsQueryExecutor};

    use super::*;

    const WRITE: &str = r#"mutation { createProbe(label: "BRIDGED") { id } }"#;

    fn run_as(tenant: Option<&str>) -> SecurityContext {
        SecurityContext::system_job(
            "p1550_fn",
            "p1550_req",
            Vec::new(),
            Vec::new(),
            tenant.map(Into::into),
        )
    }

    /// A tenant's function write lands in the tenant's schema.
    #[tokio::test]
    async fn a_tenants_function_write_lands_in_its_own_schema() {
        let Some((admin, state)) = setup().await else {
            return;
        };
        let bridge = RunAsQueryExecutor::new(ExecutorSource::of(&state), run_as(Some(TENANT)));
        bridge.execute_query(WRITE, None).await.expect("the write runs");
        assert_eq!(
            labels_in(&admin, &format!("tenant_{TENANT}")).await,
            ["BRIDGED", TENANT_ROW],
            "the tenant's"
        );
        assert_eq!(labels_in(&admin, "public").await, [PUBLIC_DECOY], "public is untouched");
    }

    /// A function of no tenant writes through the default executor.
    #[tokio::test]
    async fn a_function_of_no_tenant_writes_through_the_default_executor() {
        let Some((admin, state)) = setup().await else {
            return;
        };
        let bridge = RunAsQueryExecutor::new(ExecutorSource::of(&state), run_as(None));
        bridge.execute_query(WRITE, None).await.expect("the write runs");
        assert_eq!(labels_in(&admin, "public").await, ["BRIDGED", PUBLIC_DECOY]);
    }

    /// An unregistered or suspended tenant's write is refused, never sent to the default.
    #[tokio::test]
    async fn an_unregistered_or_suspended_tenants_function_write_is_refused() {
        let Some((admin, state)) = setup().await else {
            return;
        };
        let unknown =
            RunAsQueryExecutor::new(ExecutorSource::of(&state), run_as(Some("p1550nobody")));
        assert!(unknown.execute_query(WRITE, None).await.is_err(), "unregistered");
        state.tenant_registry().unwrap().suspend(TENANT).unwrap();
        let suspended = RunAsQueryExecutor::new(ExecutorSource::of(&state), run_as(Some(TENANT)));
        assert!(suspended.execute_query(WRITE, None).await.is_err(), "suspended");
        assert_eq!(labels_in(&admin, "public").await, [PUBLIC_DECOY], "nothing reached public");
    }

    /// A multi-tenant source's per-message tenant reaches the tenant's executor too.
    #[tokio::test]
    async fn a_per_message_tenant_reaches_its_own_executor() {
        let Some((admin, state)) = setup().await else {
            return;
        };
        let bridge = RunAsQueryExecutor::new(ExecutorSource::of(&state), run_as(None));
        let variables = json!({ fraiseql_server::query_bridge::SOURCE_TENANT_VAR: TENANT });
        bridge.execute_query(WRITE, Some(&variables)).await.expect("the write runs");
        assert_eq!(labels_in(&admin, &format!("tenant_{TENANT}")).await, ["BRIDGED", TENANT_ROW]);
    }
}

/// The suite's document compiles with no database.
#[test]
fn the_document_loads_without_a_database() {
    assert!(schema().find_type("Probe").is_some());
}
