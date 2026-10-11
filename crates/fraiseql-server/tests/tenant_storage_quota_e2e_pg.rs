//! #633: in schema-mode tenancy a tenant's storage is measured, and a tenant over its
//! `max_storage_bytes` has its mutations refused until its usage falls back under it.
//!
//! The quota is set just above what the tenant's schema weighs empty, a large row pushes it
//! over, one sweep marks it, and its writes are refused at the engine's write chokepoint,
//! on GraphQL and REST alike, with nothing written. Its reads are still served, another
//! tenant's writes are unaffected, and emptying the table and sweeping again lifts the
//! refusal.
//!
//! **Execution engine:** `PostgreSQL` · **Infrastructure:** `DATABASE_URL` ·
//! **Parallelism:** creates and drops `tenant_p633q*` → run `--test-threads=1`.
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

/// The tenant with a quota.
const QUOTA: &str = "p633qa";
/// A tenant with none, on the same server.
const FREE: &str = "p633qb";

/// What the quota leaves above the tenant's empty schema; the large row is far more.
const HEADROOM: u64 = 256 * 1024;

const WRITE: &str = r#"mutation { createProbe(label: "W") { id } }"#;

fn schema(mode: &str) -> CompiledSchema {
    let intermediate: IntermediateSchema = serde_json::from_value(json!({
        "types": [{
            "name": "Probe", "sql_source": "v_p633_probe", "is_input": false,
            "fields": [{ "name": "id", "type": "ID", "nullable": false },
                       { "name": "label", "type": "String", "nullable": false }]
        }],
        "queries": [{
            "name": "probes", "return_type": "Probe", "returns_list": true, "nullable": false,
            "sql_source": "v_p633_probe", "arguments": []
        }],
        "mutations": [{
            "name": "createProbe", "return_type": "Probe", "operation": "insert",
            "sql_source": "fn_p633_create",
            "arguments": [{ "name": "label", "type": "String", "nullable": false }]
        }],
        "security": { "tenancy": { "mode": mode, "tenant_claim": "tenant_id" } }
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

fn objects(schema: &str) -> Vec<String> {
    vec![
        format!("CREATE TABLE {schema}.tb_p633_probe (id uuid PRIMARY KEY, label text)"),
        format!(
            "CREATE VIEW {schema}.v_p633_probe AS SELECT id, jsonb_build_object('id', id, \
             'label', label) AS data FROM {schema}.tb_p633_probe"
        ),
        format!(
            "CREATE FUNCTION {schema}.fn_p633_create(p_label text) RETURNS TABLE(succeeded \
             boolean, state_changed boolean, error_class text, status_detail text, http_status \
             smallint, message text, entity_id uuid, entity_type text, entity jsonb, \
             updated_fields text[], cascade jsonb, error_detail jsonb, metadata jsonb) \
             LANGUAGE plpgsql AS $$ DECLARE v uuid := gen_random_uuid(); BEGIN INSERT INTO \
             {schema}.tb_p633_probe VALUES (v, p_label); RETURN QUERY SELECT * FROM \
             fraiseql.mutation_ok(jsonb_build_object('id', v, 'label', p_label), v, 'Probe'); \
             END $$"
        ),
    ]
}

async fn exec(adapter: &PostgresAdapter, sql: &str) {
    let _: Vec<HashMap<String, Value>> = adapter
        .execute_raw_query(sql)
        .await
        .unwrap_or_else(|e| panic!("fixture SQL `{sql}`: {e}"));
}

async fn scalar(adapter: &PostgresAdapter, sql: &str) -> i64 {
    let rows: Vec<HashMap<String, Value>> = adapter.execute_raw_query(sql).await.unwrap();
    rows[0].values().next().unwrap().as_i64().unwrap()
}

async fn rows_in(adapter: &PostgresAdapter, key: &str) -> i64 {
    scalar(adapter, &format!("SELECT count(*) AS n FROM tenant_{key}.tb_p633_probe")).await
}

/// What the tenant's schema weighs, measured as the sweep measures it.
async fn schema_bytes(adapter: &PostgresAdapter, key: &str) -> u64 {
    let n = scalar(
        adapter,
        &format!(
            "SELECT COALESCE(sum(pg_total_relation_size(c.oid)), 0)::bigint AS n FROM pg_class c \
             JOIN pg_namespace n ON n.oid = c.relnamespace WHERE n.nspname = 'tenant_{key}' AND \
             c.relkind IN ('r', 'm')"
        ),
    )
    .await;
    u64::try_from(n).unwrap()
}

fn registration(
    url: &str,
    mode: &str,
    max_storage_bytes: Option<u64>,
) -> TenantRegistrationRequest {
    TenantRegistrationRequest {
        cost_budget_per_actor: HashMap::new(),
        schema: serde_json::to_value(schema(mode)).unwrap(),
        connection: TenantPoolConfig {
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
        max_requests_per_sec: None,
        max_concurrent: None,
        max_storage_bytes,
        cost_budget: None,
        cost_budget_per_minute: None,
    }
}

async fn register(state: &AppState, key: &str, request: TenantRegistrationRequest) {
    let _ = upsert_tenant_handler(
        State(state.clone()),
        Path(key.to_string()),
        OptionalSecurityContext(None),
        axum::Json(request),
    )
    .await
    .unwrap_or_else(|e| panic!("registering {key}: {e:?}"));
}

/// Two schema-mode tenants: `QUOTA`, whose quota leaves `HEADROOM` above its empty schema,
/// and `FREE`, with none.
async fn setup() -> Option<(String, PostgresAdapter, AppState)> {
    let url = try_database_url()?;
    let admin = PostgresAdapter::new(&url).await.expect("connect to the test database");
    let (client, connection) = tokio_postgres::connect(&url, tokio_postgres::NoTls).await.unwrap();
    tokio::spawn(connection);
    client
        .batch_execute(include_str!("../../fraiseql-cli/sql/helpers/mutation_response.sql"))
        .await
        .unwrap();

    let default_executor = Arc::new(Executor::new(schema("schema"), Arc::new(admin.clone())));
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

    for key in [QUOTA, FREE] {
        exec(&admin, &format!("DROP SCHEMA IF EXISTS tenant_{key} CASCADE")).await;
        exec(&admin, &format!("CREATE SCHEMA tenant_{key}")).await;
        for stmt in objects(&format!("tenant_{key}")) {
            exec(&admin, &stmt).await;
        }
    }
    let quota = schema_bytes(&admin, QUOTA).await + HEADROOM;
    register(&state, QUOTA, registration(&url, "schema", Some(quota))).await;
    register(&state, FREE, registration(&url, "schema", None)).await;
    Some((url, admin, state))
}

/// Push `QUOTA` past its quota: one row far larger than the headroom.
async fn fill(admin: &PostgresAdapter) {
    exec(
        admin,
        &format!(
            "INSERT INTO tenant_{QUOTA}.tb_p633_probe SELECT gen_random_uuid(), \
             string_agg(md5(random()::text), '') FROM generate_series(1, 64 * 1024)"
        ),
    )
    .await;
}

async fn graphql_write(state: &AppState, key: &str) -> fraiseql_error::Result<Value> {
    let executor = state.executor_for_tenant(Some(key))?;
    executor.execute(WRITE, None).await
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
    .unwrap()
}

async fn rest(state: &AppState, request: Request<Body>) -> StatusCode {
    rest_body(state, request).await.0
}

async fn rest_body(state: &AppState, request: Request<Body>) -> (StatusCode, Value) {
    let response = router(state).oneshot(request).await.unwrap();
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    (status, serde_json::from_slice(&bytes).unwrap_or(Value::Null))
}

fn rest_post(key: &str) -> Request<Body> {
    Request::post("/rest/v1/probes")
        .header("x-tenant-id", key)
        .header("content-type", "application/json")
        .body(Body::from(json!({ "label": "W" }).to_string()))
        .unwrap()
}

/// Over its quota, a tenant's writes are refused on GraphQL and REST and nothing is
/// written; its reads are served; another tenant writes; back under, it writes again.
#[tokio::test]
async fn an_over_quota_tenant_is_refused_writes_until_it_is_back_under() {
    let Some((_url, admin, state)) = setup().await else {
        eprintln!("skipping #633: DATABASE_URL not set");
        return;
    };
    let registry = state.tenant_registry().unwrap();

    registry.sweep_storage().await;
    graphql_write(&state, QUOTA).await.expect("under its quota, it writes");

    fill(&admin).await;
    registry.sweep_storage().await;
    let before = rows_in(&admin, QUOTA).await;

    let refused = graphql_write(&state, QUOTA).await.expect_err("over its quota");
    assert_eq!(refused.status_code(), 507, "{refused}");
    assert!(refused.to_string().contains("max_storage_bytes"), "names the quota: {refused}");
    let (status, body) = rest_body(&state, rest_post(QUOTA)).await;
    assert_eq!(status, StatusCode::INSUFFICIENT_STORAGE, "{body}");
    assert_eq!(body["error"]["code"], "INSUFFICIENT_STORAGE", "{body}");
    assert!(body.to_string().contains("max_storage_bytes"), "the message survives: {body}");
    assert_eq!(rows_in(&admin, QUOTA).await, before, "nothing was written");

    let read = Request::get("/rest/v1/probes").header("x-tenant-id", QUOTA).body(Body::empty());
    assert_eq!(rest(&state, read.unwrap()).await, StatusCode::OK, "reads are served");
    graphql_write(&state, FREE).await.expect("another tenant writes");
    assert_eq!(rest(&state, rest_post(FREE)).await, StatusCode::CREATED);

    exec(&admin, &format!("TRUNCATE tenant_{QUOTA}.tb_p633_probe")).await;
    registry.sweep_storage().await;
    graphql_write(&state, QUOTA)
        .await
        .expect("back under its quota, it writes again");
}

/// A tenant is measured when it is registered: one registered over its quota is refused
/// its first write, not served until the next sweep.
#[tokio::test]
async fn a_tenant_registered_over_its_quota_is_refused_its_first_write() {
    let Some((url, admin, state)) = setup().await else {
        return;
    };
    fill(&admin).await;
    let quota = schema_bytes(&admin, FREE).await + HEADROOM;
    register(&state, QUOTA, registration(&url, "schema", Some(quota))).await;
    let before = rows_in(&admin, QUOTA).await;
    let refused = graphql_write(&state, QUOTA).await.expect_err("registered over its quota");
    assert_eq!(refused.status_code(), 507, "{refused}");
    assert_eq!(rows_in(&admin, QUOTA).await, before, "nothing was written");
}

/// A quota on a row-mode tenant has no schema to measure: refused at registration.
#[tokio::test]
async fn a_row_mode_tenant_cannot_have_a_storage_quota() {
    let Some((url, _admin, state)) = setup().await else {
        return;
    };
    let refused = upsert_tenant_handler(
        State(state.clone()),
        Path("p633qrow".to_string()),
        OptionalSecurityContext(None),
        axum::Json(registration(&url, "row", Some(1024))),
    )
    .await
    .expect_err("row mode + max_storage_bytes");
    let message = format!("{refused:?}");
    assert!(message.contains("max_storage_bytes") && message.contains("schema"), "{message}");
}

/// The old advisory key is refused by name, and the refusal names the key that replaced it.
#[test]
fn the_advisory_key_is_refused_naming_its_replacement() {
    let mut body = serde_json::to_value(json!({
        "schema": {}, "connection": { "connection_string": "postgres://x" },
        "max_storage_bytes_advisory": 1
    }))
    .unwrap();
    body["connection"]["max_connections"] = json!(1);
    let error = serde_json::from_value::<TenantRegistrationRequest>(body)
        .unwrap_err()
        .to_string();
    assert!(
        error.contains("max_storage_bytes_advisory") && error.contains("max_storage_bytes"),
        "{error}"
    );
}

/// The suite's document compiles with no database.
#[test]
fn the_document_loads_without_a_database() {
    assert!(schema("schema").find_type("Probe").is_some());
}
