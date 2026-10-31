//! A function a tenant's write triggers runs within that tenant, whoever made the write.
//!
//! Under the multi-tenant runtime an anonymous request is addressed to a tenant by its
//! `X-Tenant-ID` (or its host), and its write commits in that tenant's schema. The
//! `after:mutation` function the write triggers then writes back through its
//! `fraiseql_query` bridge. That bridge must reach the same tenant: a function triggered
//! by tenant data that reads or writes another tenant's (here the default executor's)
//! carries data across the boundary the request was confined to.
//!
//! Driven on a booted server, over HTTP, with a real `TypeScript` function, against
//! PostgreSQL in schema mode: the default executor serves `public`, the tenant serves
//! `tenant_<key>`, and both hold the same relations, so where the function's row lands says
//! which executor its bridge used.
//!
//! **Execution engine:** PostgreSQL · **Infrastructure:** `DATABASE_URL` ·
//! **Parallelism:** owns a scratch database, and boots one server: a second V8 isolate in
//! the same process is not supported, so this file holds one booted test.
#![cfg(feature = "functions-runtime-deno")]
#![allow(clippy::unwrap_used, clippy::print_stderr)] // Reason: test code — panics/skips are fine

use std::{collections::HashMap, sync::Arc, time::Duration};

use fraiseql_cli::schema::{ConvertOptions, SchemaConverter, intermediate::IntermediateSchema};
use fraiseql_core::{
    db::postgres::{PostgresAdapter, PostgresTlsConfig, ReadReplicaPolicy},
    schema::CompiledSchema,
};
use fraiseql_server::{Server, server_config::ServerConfig, tenancy::make_executor_factory};
use fraiseql_test_support::try_database_url;
use reqwest::StatusCode;
use serde_json::{Value, json};
use sqlx::PgPool;

const DB: &str = "fraiseql_function_tenant_dispatch";
const TENANT: &str = "fntenant";
const ADMIN_TOKEN: &str = "function-tenant-dispatch-admin-token-0123456789";

/// The schema both the default executor and the tenant serve: orders, and the receipt the
/// function records for each one through the bridge.
fn schema() -> CompiledSchema {
    let intermediate: IntermediateSchema = serde_json::from_value(json!({
        "types": [{
            "name": "Order", "sql_source": "v_order", "is_input": false,
            "fields": [{ "name": "id", "type": "ID", "nullable": false },
                       { "name": "label", "type": "String", "nullable": false }]
        }, {
            "name": "Receipt", "sql_source": "v_receipt", "is_input": false,
            "fields": [{ "name": "id", "type": "ID", "nullable": false },
                       { "name": "orderLabel", "type": "String", "nullable": false }]
        }],
        "queries": [{
            "name": "orders", "return_type": "Order", "returns_list": true, "nullable": false,
            "sql_source": "v_order", "arguments": []
        }],
        "mutations": [{
            "name": "createOrder", "return_type": "Order", "operation": "insert",
            "sql_source": "fn_create_order",
            "arguments": [{ "name": "label", "type": "String", "nullable": false }]
        }, {
            "name": "recordReceipt", "return_type": "Receipt", "operation": "insert",
            "sql_source": "fn_record_receipt",
            "arguments": [{ "name": "orderLabel", "type": "String", "nullable": false }]
        }],
        "security": { "tenancy": { "mode": "schema", "tenant_claim": "tenant_id" } }
    }))
    .unwrap();
    let mut schema = SchemaConverter::convert_artifact(intermediate, &ConvertOptions::default())
        .unwrap()
        .schema;
    schema.build_indexes();
    schema
}

/// The relations and write functions of one schema.
fn objects(schema: &str) -> String {
    let row = "succeeded boolean, state_changed boolean, error_class text, status_detail text, \
               http_status smallint, message text, entity_id uuid, entity_type text, entity \
               jsonb, updated_fields text[], cascade jsonb, error_detail jsonb, metadata jsonb";
    format!(
        "CREATE SCHEMA IF NOT EXISTS {schema};
         CREATE TABLE {schema}.tb_order (id uuid PRIMARY KEY, label text NOT NULL);
         CREATE TABLE {schema}.tb_receipt (id uuid PRIMARY KEY, order_label text NOT NULL);
         CREATE VIEW {schema}.v_order AS
             SELECT id, jsonb_build_object('id', id, 'label', label) AS data FROM {schema}.tb_order;
         CREATE VIEW {schema}.v_receipt AS
             SELECT id, jsonb_build_object('id', id, 'orderLabel', order_label) AS data
             FROM {schema}.tb_receipt;
         CREATE FUNCTION {schema}.fn_create_order(p_label text) RETURNS TABLE({row})
         LANGUAGE plpgsql AS $$ DECLARE v uuid := gen_random_uuid(); BEGIN
             INSERT INTO {schema}.tb_order VALUES (v, p_label);
             RETURN QUERY SELECT * FROM fraiseql.mutation_ok(
                 jsonb_build_object('id', v, 'label', p_label), v, 'Order');
         END $$;
         CREATE FUNCTION {schema}.fn_record_receipt(p_order_label text) RETURNS TABLE({row})
         LANGUAGE plpgsql AS $$ DECLARE v uuid := gen_random_uuid(); BEGIN
             INSERT INTO {schema}.tb_receipt VALUES (v, p_order_label);
             RETURN QUERY SELECT * FROM fraiseql.mutation_ok(
                 jsonb_build_object('id', v, 'orderLabel', p_order_label), v, 'Receipt');
         END $$;"
    )
}

/// Every order gets a receipt, written back through the bridge.
const RECORD_RECEIPT: &str = r"export default async (event) => {
  await Deno.core.ops.fraiseql_query(
    `mutation ($orderLabel: String!) { recordReceipt(orderLabel: $orderLabel) { id } }`,
    JSON.stringify({ orderLabel: event.new.label }),
  );
  return { recorded: true };
};
";

fn with_database(url: &str, db: &str) -> String {
    let (base, _old) = url.rsplit_once('/').expect("database URL has a path component");
    format!("{base}/{db}")
}

async fn scratch_pool(admin_url: &str) -> PgPool {
    let admin = PgPool::connect(admin_url).await.expect("connect to admin database");
    sqlx::raw_sql(&format!("DROP DATABASE IF EXISTS {DB} WITH (FORCE)"))
        .execute(&admin)
        .await
        .expect("drop scratch database");
    sqlx::raw_sql(&format!("CREATE DATABASE {DB}"))
        .execute(&admin)
        .await
        .expect("create scratch database");
    admin.close().await;
    PgPool::connect(&with_database(admin_url, DB))
        .await
        .expect("connect to scratch database")
}

/// The receipts recorded in `schema`.
async fn receipts(pool: &PgPool, schema: &str) -> Vec<String> {
    sqlx::query_scalar(&format!("SELECT order_label FROM {schema}.tb_receipt ORDER BY 1"))
        .fetch_all(pool)
        .await
        .unwrap()
}

#[tokio::test]
async fn an_anonymous_tenant_writes_function_runs_within_that_tenant() {
    let Some(url) = try_database_url() else {
        eprintln!("SKIP: DATABASE_URL not set");
        return;
    };
    let pool = scratch_pool(&url).await;
    let scratch_url = with_database(&url, DB);
    sqlx::raw_sql(include_str!("../../fraiseql-cli/sql/helpers/mutation_response.sql"))
        .execute(&pool)
        .await
        .unwrap();
    // Every mutation writes its change-log row in the same transaction.
    sqlx::raw_sql(include_str!("../../fraiseql-cli/sql/helpers/entity_change_log_contract.sql"))
        .execute(&pool)
        .await
        .unwrap();
    sqlx::raw_sql(&objects("public")).execute(&pool).await.unwrap();
    sqlx::raw_sql(&objects(&format!("tenant_{TENANT}")))
        .execute(&pool)
        .await
        .unwrap();

    let modules = tempfile::tempdir().unwrap();
    std::fs::write(modules.path().join("recordReceipt.ts"), RECORD_RECEIPT).unwrap();
    let functions = serde_json::from_value(json!({
        "module_dir": modules.path(),
        "definitions": [{
            "name": "recordReceipt", "trigger": "after:mutation:Order", "runtime": "Deno",
            "run_as": { "roles": ["receipt_writer"] }
        }]
    }))
    .unwrap();
    let mut config = ServerConfig {
        cors_enabled: false,
        // A schema-mode tenancy refuses a response cache it cannot key by tenant.
        cache_enabled: false,
        admin_api_enabled: true,
        admin_token: Some(ADMIN_TOKEN.to_string()),
        database_url: scratch_url.clone(),
        ..ServerConfig::default()
    };
    config.tenancy.runtime.enabled = true;
    let adapter = Arc::new(PostgresAdapter::new(&scratch_url).await.expect("adapter"));
    let server = Box::pin(Server::new(config, schema(), adapter, Some(pool.clone())))
        .await
        .expect("server boots")
        .with_functions_config(Some(functions))
        .with_tenant_executor_factory(make_executor_factory::<PostgresAdapter>(
            PostgresTlsConfig::default(),
            ReadReplicaPolicy::default(),
            fraiseql_core::db::postgres::VectorScanConfig::default(),
        ));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let base = format!("http://127.0.0.1:{}", listener.local_addr().unwrap().port());
    let (stop, rx) = tokio::sync::oneshot::channel::<()>();
    let handle = tokio::spawn(async move {
        server
            .serve_on_listener(listener, async {
                let _ = rx.await;
            })
            .await
    });
    let client = reqwest::Client::new();
    let mut serving = false;
    for _ in 0..100 {
        if client.get(format!("{base}/health")).send().await.is_ok() {
            serving = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    assert!(serving, "the server answers /health");

    let registered = client
        .put(format!("{base}/api/v1/admin/tenants/{TENANT}"))
        .bearer_auth(ADMIN_TOKEN)
        .json(&json!({
            "schema": serde_json::to_value(schema()).unwrap(),
            "connection": { "connection_string": scratch_url, "max_connections": 2 },
        }))
        .send()
        .await
        .unwrap();
    let status = registered.status();
    assert!(
        status.is_success(),
        "tenant registration: {status} {}",
        registered.text().await.unwrap()
    );

    // No credential: the request names its tenant, as a public per-tenant surface does.
    let written = client
        .post(format!("{base}/graphql"))
        .header("x-tenant-id", TENANT)
        .json(&json!({ "query": r#"mutation { createOrder(label: "ANON-1") { id } }"# }))
        .send()
        .await
        .unwrap();
    let status = written.status();
    let body: Value = written.json().await.unwrap();
    assert_eq!(status, StatusCode::OK, "the anonymous write: {body}");
    assert!(body.get("errors").is_none(), "the anonymous write commits: {body}");
    let orders: Vec<String> =
        sqlx::query_scalar(&format!("SELECT label FROM tenant_{TENANT}.tb_order"))
            .fetch_all(&pool)
            .await
            .unwrap();
    assert_eq!(orders, ["ANON-1"], "the write committed in the tenant's schema");

    // The function runs after the response: wait for its receipt to land somewhere.
    let tenant_schema = format!("tenant_{TENANT}");
    let mut landed: HashMap<&str, Vec<String>> = HashMap::new();
    for _ in 0..150 {
        landed.insert("tenant", receipts(&pool, &tenant_schema).await);
        landed.insert("default", receipts(&pool, "public").await);
        if landed.values().any(|r| !r.is_empty()) {
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }

    let _ = stop.send(());
    let _ = handle.await;
    pool.close().await;

    assert!(
        landed["default"].is_empty(),
        "the function triggered by the tenant's write wrote to the default executor: {landed:?}"
    );
    assert_eq!(
        landed["tenant"],
        ["ANON-1"],
        "the function triggered by the tenant's write records its receipt in that tenant"
    );
}
