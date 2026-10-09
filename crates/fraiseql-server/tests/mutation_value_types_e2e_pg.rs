//! A mutation value of the wrong type is a validation error before any SQL (#1528).
//!
//! Queries checked each argument value against its declared type; mutations checked only
//! argument names, enum membership and required fields, so `name: String` given an object
//! reached the SQL function as JSON text, and `qty: Int` given `"3"` surfaced as a database
//! error, if at all. Each case here sends a wrong-typed value to a real function that records
//! every call it receives, and asserts a validation error naming the field and the type, and
//! an empty call log: the function never ran.
//!
//! **Execution engine:** `PostgreSQL` · **Infrastructure:** `DATABASE_URL` ·
//! **Parallelism:** creates and drops its own `mutation_value_types` schema → run
//! `--test-threads=1`.
#![cfg(all(feature = "rest", feature = "mcp"))]
#![allow(clippy::unwrap_used, clippy::panic, clippy::print_stderr)] // Reason: test code — panics and skip diagnostics are acceptable

use std::sync::Arc;

use fraiseql_core::{
    db::postgres::PostgresAdapter,
    prelude::DatabaseAdapter as _,
    runtime::Executor,
    schema::{
        ArgumentDefinition, CompiledSchema, FieldDefinition, FieldType, InputFieldDefinition,
        InputObjectDefinition, McpConfig, MutationDefinition, MutationOperation, QueryDefinition,
        RestConfig, TypeDefinition,
    },
};
use fraiseql_server::{
    Server, mcp::handler::FraiseQLMcpService, routes::graphql::AppState,
    server_config::ServerConfig,
};
use fraiseql_test_support::try_database_url;
use serde_json::{Value, json};

const SCHEMA: &str = "mutation_value_types";

/// `tb_call` logs every call either function receives; `v_thing` is the return type's view.
async fn provision(adapter: &PostgresAdapter) {
    let mut stmts = vec![
        "CREATE SCHEMA IF NOT EXISTS app".to_string(),
        "DO $$ BEGIN CREATE TYPE app.mutation_error_class AS ENUM ('validation','conflict',\
         'not_found','unauthorized','forbidden','internal','transaction_failed','timeout',\
         'rate_limited','service_unavailable'); EXCEPTION WHEN duplicate_object THEN NULL; END $$;"
            .to_string(),
        "DO $$ BEGIN CREATE TYPE app.mutation_response AS (succeeded BOOLEAN, state_changed \
         BOOLEAN, error_class app.mutation_error_class, status_detail TEXT, http_status \
         SMALLINT, message TEXT, entity_id UUID, entity_type TEXT, entity JSONB, \
         updated_fields TEXT[], cascade JSONB, error_detail JSONB, metadata JSONB); \
         EXCEPTION WHEN duplicate_object THEN NULL; END $$;"
            .to_string(),
        format!("DROP SCHEMA IF EXISTS {SCHEMA} CASCADE"),
        format!("CREATE SCHEMA {SCHEMA}"),
        format!("CREATE TABLE {SCHEMA}.tb_call (via text, args jsonb)"),
        format!("CREATE TABLE {SCHEMA}.tb_thing (id uuid PRIMARY KEY, name text)"),
        format!(
            "CREATE VIEW {SCHEMA}.v_thing AS SELECT id, jsonb_build_object('id', id, 'name', \
             name) AS data FROM {SCHEMA}.tb_thing"
        ),
        format!(
            "CREATE FUNCTION {SCHEMA}.fn_create_thing(p_name text, p_qty int, p_ref uuid) \
             RETURNS app.mutation_response LANGUAGE plpgsql AS $$ \
             DECLARE v app.mutation_response; n uuid := gen_random_uuid(); BEGIN \
             INSERT INTO {SCHEMA}.tb_call VALUES ('thing', jsonb_build_array(p_name, p_qty, p_ref)); \
             INSERT INTO {SCHEMA}.tb_thing VALUES (n, p_name); \
             v.succeeded := true; v.state_changed := true; v.entity_type := 'Thing'; \
             v.entity_id := n; v.entity := jsonb_build_object('id', n, 'name', p_name); \
             RETURN v; END; $$"
        ),
        format!(
            "CREATE FUNCTION {SCHEMA}.fn_create_order(p_name text, p_lines jsonb) \
             RETURNS app.mutation_response LANGUAGE plpgsql AS $$ \
             DECLARE v app.mutation_response; n uuid := gen_random_uuid(); BEGIN \
             INSERT INTO {SCHEMA}.tb_call VALUES ('order', jsonb_build_array(p_name, p_lines)); \
             INSERT INTO {SCHEMA}.tb_thing VALUES (n, p_name); \
             v.succeeded := true; v.state_changed := true; v.entity_type := 'Thing'; \
             v.entity_id := n; v.entity := jsonb_build_object('id', n, 'name', p_name); \
             RETURN v; END; $$"
        ),
    ];
    stmts.extend(fraiseql_test_support::changelog::entity_change_log_provision_statements());
    for stmt in stmts {
        adapter.execute_raw_query(&stmt).await.unwrap_or_else(|e| panic!("{stmt}: {e}"));
    }
}

fn input_field(name: &str, field_type: &str) -> InputFieldDefinition {
    InputFieldDefinition::new(name, field_type)
}

/// `createThing(name: String, qty: Int, ref: UUID)`, flat; `createOrder(input: OrderInput!)`
/// with `OrderInput { name: String!, lines: [LineInput!] }` and
/// `LineInput { sku: String!, qty: Int! }`.
fn schema() -> CompiledSchema {
    let mut schema = CompiledSchema::new();
    let mut thing = TypeDefinition::new("Thing", format!("{SCHEMA}.v_thing"));
    thing.fields = vec![
        FieldDefinition::new("id", FieldType::Id),
        FieldDefinition::nullable("name", FieldType::String),
    ];
    schema.types.push(thing);
    schema.queries.push(
        QueryDefinition::new("things", "Thing")
            .returning_list()
            .with_sql_source(format!("{SCHEMA}.v_thing")),
    );
    let mut by_id =
        QueryDefinition::new("thing", "Thing").with_sql_source(format!("{SCHEMA}.v_thing"));
    by_id.arguments = vec![ArgumentDefinition::new("id", FieldType::Id)];
    schema.queries.push(by_id);

    let mut line = InputObjectDefinition::new("LineInput");
    line.fields = vec![input_field("sku", "String!"), input_field("qty", "Int!")];
    let mut order = InputObjectDefinition::new("OrderInput");
    order.fields = vec![
        input_field("name", "String!"),
        input_field("lines", "[LineInput!]"),
    ];
    schema.input_types.extend([line, order]);

    let mut create = MutationDefinition::new("createThing", "Thing");
    create.sql_source = Some(format!("{SCHEMA}.fn_create_thing"));
    create.operation = MutationOperation::Insert {
        table: "tb_thing".to_string(),
    };
    create.arguments = vec![
        ArgumentDefinition::optional("name", FieldType::String),
        ArgumentDefinition::optional("qty", FieldType::Int),
        ArgumentDefinition::optional("ref", FieldType::Uuid),
    ];
    let mut order_mutation = MutationDefinition::new("createOrder", "Thing");
    order_mutation.sql_source = Some(format!("{SCHEMA}.fn_create_order"));
    order_mutation.operation = MutationOperation::Insert {
        table: "tb_thing".to_string(),
    };
    order_mutation.arguments = vec![ArgumentDefinition::new(
        "input",
        FieldType::Object("OrderInput".to_string()),
    )];
    // `POST /things` is `createThing`'s.
    order_mutation.rest_path = Some("/orders".to_string());
    schema.mutations.extend([create, order_mutation]);
    schema.rest_config = Some(RestConfig {
        enabled: true,
        ..RestConfig::default()
    });
    schema.build_indexes();
    schema
}

/// Every call the functions received, in order.
async fn calls(url: &str) -> Vec<Value> {
    let adapter = PostgresAdapter::new(url).await.unwrap();
    adapter
        .execute_raw_query(&format!("SELECT via, args FROM {SCHEMA}.tb_call"))
        .await
        .unwrap()
        .into_iter()
        .map(|row| json!({ "via": row["via"], "args": row["args"] }))
        .collect()
}

async fn clear_calls(url: &str) {
    let adapter = PostgresAdapter::new(url).await.unwrap();
    adapter.execute_raw_query(&format!("TRUNCATE {SCHEMA}.tb_call")).await.unwrap();
}

/// A running server; dropping it shuts the server down.
struct Running {
    base:      String,
    _shutdown: tokio::sync::oneshot::Sender<()>,
}

async fn serve(url: &str) -> Running {
    let adapter = Arc::new(PostgresAdapter::new(url).await.unwrap());
    provision(&adapter).await;
    let config = ServerConfig {
        database_url: url.to_string(),
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
    Running {
        base:      format!("http://127.0.0.1:{port}"),
        _shutdown: tx,
    }
}

async fn graphql(server: &Running, query: &str, variables: &Value) -> Value {
    reqwest::Client::new()
        .post(format!("{}/graphql", server.base))
        .json(&json!({ "query": query, "variables": variables }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap()
}

/// The first error's message, asserting the request was refused as a validation error.
fn refusal(body: &Value) -> String {
    let message = body["errors"][0]["message"].as_str().unwrap_or_else(|| panic!("{body}"));
    assert!(
        body["data"].is_null()
            || body["data"].as_object().is_some_and(|d| d.values().all(Value::is_null)),
        "{body}"
    );
    message.to_string()
}

const CREATE_THING: &str = "mutation($name: JSON, $qty: JSON, $ref: JSON) { \
                            createThing(name: $name, qty: $qty, ref: $ref) { id } }";

/// Top-level scalars: each wrong value is refused naming the argument and its type, and the
/// function never runs. A right value runs it (the control).
#[tokio::test]
async fn a_wrong_typed_mutation_argument_is_refused_before_sql() {
    let Some(url) = try_database_url() else {
        eprintln!("skipping #1528: DATABASE_URL not set");
        return;
    };
    let server = serve(&url).await;
    for (variables, argument, declared) in [
        (json!({ "name": { "a": 1 } }), "name", "String"),
        (json!({ "name": 3 }), "name", "String"),
        (json!({ "name": ["a"] }), "name", "String"),
        (json!({ "qty": "3" }), "qty", "Int"),
        (json!({ "qty": 3.5 }), "qty", "Int"),
        (json!({ "ref": "x" }), "ref", "UUID"),
    ] {
        let body = graphql(&server, CREATE_THING, &variables).await;
        let message = refusal(&body);
        assert!(
            message.contains(&format!("`{argument}`"))
                && message.contains(&format!("`{declared}`")),
            "{variables} names {argument} and {declared}: {message}"
        );
        assert_eq!(calls(&url).await, Vec::<Value>::new(), "{variables}: the function never ran");
    }

    let ok = graphql(
        &server,
        CREATE_THING,
        &json!({ "name": "n", "qty": 3, "ref": "3f2504e0-4f89-11d3-9a0c-0305e82c3301" }),
    )
    .await;
    assert!(ok.get("errors").is_none(), "the control runs: {ok}");
    assert_eq!(calls(&url).await.len(), 1, "the control's call is logged");
    clear_calls(&url).await;
}

const CREATE_ORDER: &str = "mutation($input: OrderInput!) { createOrder(input: $input) { id } }";

/// Input objects: a wrong-typed field inside a list of inputs, at depth 2, and a field the
/// input type does not declare.
#[tokio::test]
async fn a_wrong_typed_input_field_is_refused_before_sql() {
    let Some(url) = try_database_url() else {
        return;
    };
    let server = serve(&url).await;
    for (input, path, expect) in [
        (
            json!({ "name": "o", "lines": [{ "sku": "a", "qty": 1 }, { "sku": "b", "qty": "2" }] }),
            "lines[1].qty",
            "`Int`",
        ),
        (json!({ "name": 7, "lines": [] }), "name", "`String`"),
        (
            json!({ "name": "o", "lines": [{ "sku": "a", "qty": 1, "colour": "red" }] }),
            "lines[0].colour",
            "declares no such field",
        ),
        (json!({ "name": "o", "extra": true }), "extra", "declares no such field"),
    ] {
        let body = graphql(&server, CREATE_ORDER, &json!({ "input": input })).await;
        let message = refusal(&body);
        assert!(
            message.contains(&format!("`input.{path}`")) && message.contains(expect),
            "{input} names input.{path} ({expect}): {message}"
        );
        assert_eq!(calls(&url).await, Vec::<Value>::new(), "{input}: the function never ran");
    }

    let ok = graphql(
        &server,
        CREATE_ORDER,
        &json!({ "input": { "name": "o", "lines": [{ "sku": "a", "qty": 1 }] } }),
    )
    .await;
    assert!(ok.get("errors").is_none(), "the control runs: {ok}");
    assert_eq!(calls(&url).await.len(), 1, "the control's call is logged");
    clear_calls(&url).await;
}

/// REST and MCP meet the same seam: a wrong-typed body field or tool argument is refused and
/// the function never runs.
#[tokio::test]
async fn rest_and_mcp_writes_are_type_checked() {
    let Some(url) = try_database_url() else {
        return;
    };
    let server = serve(&url).await;
    let response = reqwest::Client::new()
        .post(format!("{}/rest/v1/things", server.base))
        .json(&json!({ "name": "n", "qty": "3" }))
        .send()
        .await
        .unwrap();
    let status = response.status();
    let text = response.text().await.unwrap();
    assert!(
        status.is_client_error() && text.contains("`qty`") && text.contains("`Int`"),
        "REST: {status} {text}"
    );
    assert_eq!(calls(&url).await, Vec::<Value>::new(), "REST: the function never ran");

    let adapter = Arc::new(PostgresAdapter::new(&url).await.unwrap());
    let service = FraiseQLMcpService::new(
        AppState::new(Arc::new(Executor::new(schema(), adapter))),
        McpConfig {
            enabled: true,
            require_auth: false,
            read_only: false,
            ..McpConfig::default()
        },
    );
    let arguments = json!({ "name": { "a": 1 } });
    let result = service
        .call_tool_authenticated(
            "createThing",
            arguments.as_object(),
            None,
            "mcp-types".to_string(),
            &axum::http::HeaderMap::new(),
        )
        .await;
    let text = result
        .content
        .first()
        .and_then(|c| c.as_text().map(|t| t.text.clone()))
        .unwrap_or_default();
    assert_eq!(result.is_error, Some(true), "MCP refuses: {text}");
    assert!(text.contains("`name`") && text.contains("`String`"), "MCP: {text}");
    assert_eq!(calls(&url).await, Vec::<Value>::new(), "MCP: the function never ran");
}
