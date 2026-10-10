//! #1397: a cascade mutation's declared success fields are served on its payload, from the
//! function's `mutation_response.result`.
//!
//! `createOrder` re-attaches orphaned lines and says how many: its function returns
//! `result = {"recovered_items": 3, "recovery": "FULL", "audit_note": …}` through
//! `fraiseql.mutation_ok_result(…)`, in a row type of its own that declares `result jsonb`
//! after the 13 columns, and the client reads `recoveredItems` next to `entity`.
//! `audit_note` is in `result` but declared nowhere, so nothing serves it.
//!
//! The other functions get `result` wrong, each one way: the non-null field missing, a value
//! of the wrong JSON type, an enum value the enum does not have, and a 13-column row with no
//! `result` at all (for a mutation whose only success field is nullable, so the row would
//! otherwise be served as a null). Each is the function's contract error: the write is rolled
//! back and counted, whatever the client selected.
//!
//! Served on every transport that serves the payload: GraphQL; REST, whose write route for a
//! cascade mutation is `POST /<payload resource>` with every leaf field selected; MCP, the
//! same leaves. gRPC answers a mutation with `success`/`id`/`error` only, the payload with no
//! field of it, so it serves none (and advertises none).
//!
//! **Execution engine:** `PostgreSQL` · **Infrastructure:** `DATABASE_URL` ·
//! **Parallelism:** drops and recreates its own `p1397_success` schema; installs the helper
//! library (`CREATE OR REPLACE`, the version this tree ships) → run `--test-threads=1`.
#![cfg(all(feature = "rest", feature = "mcp"))]
#![allow(clippy::unwrap_used, clippy::panic, clippy::print_stderr)] // Reason: test code

use std::sync::Arc;

use fraiseql_cli::schema::{ConvertOptions, SchemaConverter, intermediate::IntermediateSchema};
use fraiseql_core::{
    db::postgres::PostgresAdapter, prelude::DatabaseAdapter as _, schema::CompiledSchema,
};
use fraiseql_server::{Server, server_config::ServerConfig};
use fraiseql_test_support::try_database_url;
use serde_json::{Value, json};

const SCHEMA: &str = "p1397_success";
/// The value `result` carries that no field declares.
const UNDECLARED: &str = "never-served-audit-note";

/// A function that inserts an order and returns `result` built by `result_sql`, in the
/// 14-column row.
fn function(name: &str, result_sql: &str) -> String {
    format!(
        "CREATE FUNCTION {SCHEMA}.fn_{name}(p_total int) RETURNS SETOF \
         {SCHEMA}.mutation_response_result \
         LANGUAGE plpgsql AS $$ DECLARE v_id uuid := gen_random_uuid(); BEGIN \
         INSERT INTO {SCHEMA}.tb_order VALUES (v_id, p_total); \
         RETURN QUERY SELECT * FROM fraiseql.mutation_ok_result({result_sql}, \
         jsonb_build_object('id', v_id, 'total', p_total), v_id, 'Order'); END; $$"
    )
}

/// A function that inserts an order and returns the 13-column row, which has no `result`.
fn thirteen_column_function(name: &str) -> String {
    format!(
        "CREATE FUNCTION {SCHEMA}.fn_{name}(p_total int) RETURNS SETOF \
         {SCHEMA}.mutation_response \
         LANGUAGE plpgsql AS $$ DECLARE v_id uuid := gen_random_uuid(); BEGIN \
         INSERT INTO {SCHEMA}.tb_order VALUES (v_id, p_total); \
         RETURN QUERY SELECT * FROM fraiseql.mutation_ok(jsonb_build_object('id', v_id, \
         'total', p_total), v_id, 'Order'); END; $$"
    )
}

async fn provision(url: &str) {
    let (client, connection) = tokio_postgres::connect(url, tokio_postgres::NoTls).await.unwrap();
    tokio::spawn(connection);
    client
        .batch_execute(include_str!("../../fraiseql-cli/sql/helpers/mutation_response.sql"))
        .await
        .unwrap();
    let adapter = PostgresAdapter::new(url).await.unwrap();
    let mut stmts = vec![
        format!("DROP SCHEMA IF EXISTS {SCHEMA} CASCADE"),
        format!("CREATE SCHEMA {SCHEMA}"),
        // Its own composites, the shapes the helpers return (`error_class` text): the shared
        // `app.mutation_response` another suite created may carry an enum there.
        format!(
            "CREATE TYPE {SCHEMA}.mutation_response AS (succeeded BOOLEAN, state_changed \
             BOOLEAN, error_class TEXT, status_detail TEXT, http_status SMALLINT, message TEXT, \
             entity_id UUID, entity_type TEXT, entity JSONB, updated_fields TEXT[], cascade \
             JSONB, error_detail JSONB, metadata JSONB)"
        ),
        format!(
            "CREATE TYPE {SCHEMA}.mutation_response_result AS (succeeded BOOLEAN, \
             state_changed BOOLEAN, error_class TEXT, status_detail TEXT, http_status SMALLINT, \
             message TEXT, entity_id UUID, entity_type TEXT, entity JSONB, updated_fields \
             TEXT[], cascade JSONB, error_detail JSONB, metadata JSONB, result JSONB)"
        ),
        format!("CREATE TABLE {SCHEMA}.tb_order (id uuid PRIMARY KEY, total int)"),
        format!(
            "CREATE VIEW {SCHEMA}.v_order AS SELECT id, jsonb_build_object('id', id, 'total', \
             total) AS data FROM {SCHEMA}.tb_order"
        ),
        function(
            "create_order",
            &format!(
                "jsonb_build_object('recovered_items', 3, 'recovery', 'FULL', 'audit_note', \
                 '{UNDECLARED}')"
            ),
        ),
        function("create_order_missing", "jsonb_build_object('recovery', 'FULL')"),
        function("create_order_wrong_type", "jsonb_build_object('recovered_items', 'three')"),
        function(
            "create_order_wrong_enum",
            "jsonb_build_object('recovered_items', 1, 'recovery', 'MOSTLY')",
        ),
        thirteen_column_function("create_order_no_column"),
    ];
    stmts.extend(fraiseql_test_support::changelog::entity_change_log_provision_statements());
    for stmt in stmts {
        adapter.execute_raw_query(&stmt).await.unwrap_or_else(|e| panic!("{stmt}: {e}"));
    }
}

/// The schema an SDK authors: four cascade mutations of one shape, and one whose only
/// success field is nullable, compiled.
fn schema() -> CompiledSchema {
    let recovered_items = json!({ "name": "recoveredItems", "type": "Int", "nullable": false });
    let recovery = json!({ "name": "recovery", "type": "Recovery", "nullable": true });
    let declaring = |name: &str, function: &str, success_fields: Value| {
        json!({
            "name": name, "return_type": "Order", "operation": "insert", "cascade": true,
            "sql_source": format!("{SCHEMA}.fn_{function}"),
            "arguments": [{ "name": "total", "type": "Int", "nullable": false }],
            "success_fields": success_fields
        })
    };
    let mutation =
        |name: &str, function: &str| declaring(name, function, json!([recovered_items, recovery]));
    let intermediate: IntermediateSchema = serde_json::from_value(json!({
        "types": [{
            "name": "Order", "sql_source": format!("{SCHEMA}.v_order"), "is_input": false,
            "fields": [{ "name": "id", "type": "ID", "nullable": false },
                       { "name": "total", "type": "Int", "nullable": false }]
        }],
        "enums": [{ "name": "Recovery", "values": [{ "name": "FULL" }, { "name": "PARTIAL" }] }],
        "queries": [{
            "name": "orders", "return_type": "Order", "returns_list": true, "nullable": false,
            "sql_source": format!("{SCHEMA}.v_order"), "arguments": []
        }],
        "mutations": [
            mutation("createOrder", "create_order"),
            mutation("createOrderMissing", "create_order_missing"),
            mutation("createOrderWrongType", "create_order_wrong_type"),
            mutation("createOrderWrongEnum", "create_order_wrong_enum"),
            declaring("createOrderNoColumn", "create_order_no_column", json!([recovery]))
        ]
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

struct Running {
    base:      String,
    url:       String,
    _shutdown: tokio::sync::oneshot::Sender<()>,
}

async fn serve() -> Option<Running> {
    let url = try_database_url()?;
    provision(&url).await;
    let adapter = Arc::new(PostgresAdapter::new(&url).await.unwrap());
    let config = ServerConfig {
        database_url: url.clone(),
        cors_enabled: false,
        ..ServerConfig::default()
    };
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let server = Box::pin(Server::new(config, schema(), adapter, None))
        .await
        .unwrap()
        .with_rest_write_surface();
    let (tx, rx) = tokio::sync::oneshot::channel::<()>();
    tokio::spawn(async move {
        server
            .serve_on_listener(listener, async {
                let _ = rx.await;
            })
            .await
    });
    tokio::time::sleep(std::time::Duration::from_millis(300)).await;
    Some(Running {
        base: format!("http://127.0.0.1:{port}"),
        url,
        _shutdown: tx,
    })
}

async fn graphql(server: &Running, mutation: &str, selection: &str) -> Value {
    let body: Value = reqwest::Client::new()
        .post(format!("{}/graphql", server.base))
        .json(&json!({ "query": format!("mutation {{ {mutation}(total: 9) {selection} }}") }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert!(
        !body.to_string().contains(UNDECLARED),
        "an undeclared key is never served: {body}"
    );
    body
}

async fn orders(server: &Running) -> i64 {
    let (client, connection) =
        tokio_postgres::connect(&server.url, tokio_postgres::NoTls).await.unwrap();
    tokio::spawn(connection);
    client
        .query_one(&format!("SELECT count(*) FROM {SCHEMA}.tb_order"), &[])
        .await
        .unwrap()
        .get(0)
}

const PAYLOAD: &str = "{ ... on CreateOrderPayload { entity { id total } recoveredItems recovery } \
                       ... on CreateOrderMissingPayload { recoveredItems recovery } \
                       ... on CreateOrderWrongTypePayload { recoveredItems } \
                       ... on CreateOrderWrongEnumPayload { recoveredItems recovery } \
                       ... on CreateOrderNoColumnPayload { recovery } }";

/// The issue's mutation: the success fields next to `entity`, read from `result`.
#[tokio::test]
async fn the_declared_success_fields_are_served_next_to_the_entity() {
    let Some(server) = serve().await else {
        eprintln!("skipping #1397: DATABASE_URL not set");
        return;
    };
    let body = graphql(&server, "createOrder", PAYLOAD).await;
    let payload = &body["data"]["createOrder"];
    assert_eq!(payload["recoveredItems"], 3, "{body}");
    assert_eq!(payload["recovery"], "FULL", "{body}");
    assert_eq!(payload["entity"]["total"], 9, "{body}");
    assert!(body.get("errors").is_none(), "{body}");
}

/// A value the declared type cannot hold, or no value for a non-null field, is the
/// function's contract error: refused, and the write rolled back, as an off-contract stamp
/// is. Not a field error after the commit: REST has no partial answer to carry one in, and a
/// committed write answered with an error invites a retry.
#[tokio::test]
async fn a_success_field_of_the_wrong_type_refuses_the_write() {
    let Some(server) = serve().await else {
        return;
    };
    for (mutation, field) in [
        ("createOrderMissing", "recoveredItems"),
        ("createOrderWrongType", "recoveredItems"),
        ("createOrderWrongEnum", "recovery"),
        ("createOrderNoColumn", "recovery"),
    ] {
        let before = orders(&server).await;
        let counted = fraiseql_core::runtime::mutation_contract_errors();
        let body = graphql(&server, mutation, PAYLOAD).await;
        let errors = body["errors"].as_array().unwrap_or_else(|| panic!("{mutation}: {body}"));
        assert!(
            errors.iter().any(|e| e["message"].as_str().unwrap_or("").contains(field)),
            "{mutation}: names `{field}`: {body}"
        );
        assert_eq!(orders(&server).await, before, "{mutation}: the write was rolled back");
        assert!(
            fraiseql_core::runtime::mutation_contract_errors() > counted,
            "{mutation}: counted as a contract error"
        );
    }
    // What commits does not depend on the selection: `recovery` unselected, still refused.
    let before = orders(&server).await;
    let body = graphql(
        &server,
        "createOrderWrongEnum",
        "{ ... on CreateOrderWrongEnumPayload { entity { id } } }",
    )
    .await;
    assert!(body["errors"].to_string().contains("recovery"), "{body}");
    assert_eq!(orders(&server).await, before, "unselected, and still rolled back");
}

/// The suite's document compiles with no database.
#[test]
fn the_document_loads_without_a_database() {
    let schema = schema();
    assert!(schema.find_type("CreateOrderPayload").is_some());
}

/// REST: the cascade mutation's write route answers its leaves, the success fields among
/// them; a wrong one refuses the write.
#[tokio::test]
async fn rest_serves_the_success_fields_and_refuses_a_wrong_one() {
    let Some(server) = serve().await else {
        return;
    };
    let post = |resource: &str| {
        reqwest::Client::new()
            .post(format!("{}/rest/v1/{resource}", server.base))
            .json(&json!({ "total": 9 }))
            .send()
    };
    let response = post("create_order_payloads").await.unwrap();
    assert_eq!(response.status(), 201);
    let body: Value = response.json().await.unwrap();
    assert!(!body.to_string().contains(UNDECLARED), "{body}");
    assert_eq!(
        body["data"]["createOrder"],
        json!({ "recoveredItems": 3, "recovery": "FULL" }),
        "{body}"
    );

    for resource in [
        "create_order_missing_payloads",
        "create_order_wrong_type_payloads",
        "create_order_wrong_enum_payloads",
        "create_order_no_column_payloads",
    ] {
        let before = orders(&server).await;
        let response = post(resource).await.unwrap();
        let status = response.status();
        let body = response.text().await.unwrap();
        assert!(!status.is_success(), "{resource}: {status} {body}");
        assert_eq!(orders(&server).await, before, "{resource}: the write was rolled back");
    }
}

/// MCP: the tool's answer carries the success fields, and a wrong one is an error result.
#[tokio::test]
async fn mcp_serves_the_success_fields_and_refuses_a_wrong_one() {
    use fraiseql_core::{runtime::Executor, schema::McpConfig};
    use fraiseql_server::{mcp::handler::FraiseQLMcpService, routes::graphql::AppState};

    let Some(url) = try_database_url() else {
        return;
    };
    provision(&url).await;
    let adapter = PostgresAdapter::new(&url).await.unwrap();
    let state = AppState::new(Arc::new(Executor::new(schema(), Arc::new(adapter))));
    let service = FraiseQLMcpService::new(
        state,
        McpConfig {
            enabled: true,
            require_auth: false,
            ..McpConfig::default()
        },
    );
    let call = |tool: &'static str| {
        let service = &service;
        async move {
            let mut args = serde_json::Map::new();
            args.insert("total".to_string(), json!(9));
            service
                .call_tool_authenticated(
                    tool,
                    Some(&args),
                    None,
                    format!("p1397-{tool}"),
                    &axum::http::HeaderMap::new(),
                )
                .await
        }
    };
    let text = |result: &rmcp::model::CallToolResult| {
        result
            .content
            .iter()
            .filter_map(|c| c.as_text().map(|t| t.text.clone()))
            .collect::<String>()
    };

    let result = call("createOrder").await;
    assert_ne!(result.is_error, Some(true), "{:?}", result.content);
    let served: Value = serde_json::from_str(&text(&result)).unwrap();
    assert!(!served.to_string().contains(UNDECLARED), "{served}");
    assert_eq!(served["data"]["createOrder"]["recoveredItems"], 3, "{served}");
    assert_eq!(served["data"]["createOrder"]["recovery"], "FULL", "{served}");

    for tool in [
        "createOrderMissing",
        "createOrderWrongType",
        "createOrderWrongEnum",
        "createOrderNoColumn",
    ] {
        let result = call(tool).await;
        assert_eq!(result.is_error, Some(true), "{tool}: {}", text(&result));
    }
}
