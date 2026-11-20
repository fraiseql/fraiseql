//! An input object field's declared default reaches the function when the field is
//! omitted, and never when it is sent, null included (GraphQL, Input Objects → Input
//! Coercion).
//!
//! Each function records the arguments it received in `tb_call`, so a test reads what
//! the database saw rather than what the response says.

#![cfg(feature = "rest")]
#![allow(clippy::unwrap_used, clippy::panic)] // Reason: test code — panics are acceptable

use std::sync::Arc;

use fraiseql_core::{
    db::postgres::PostgresAdapter,
    prelude::DatabaseAdapter as _,
    schema::{
        ArgumentDefinition, CompiledSchema, FieldDefinition, FieldType, InputFieldDefinition,
        InputObjectDefinition, InputStyle, MutationDefinition, MutationOperation, QueryDefinition,
        RestConfig, SourceKind, TypeDefinition,
    },
};
use fraiseql_server::{Server, server_config::ServerConfig};
use fraiseql_test_support::try_database_url;
use serde_json::{Value, json};

const SCHEMA: &str = "input_field_defaults";

/// The response row every function returns.
const RESPOND: &str = "v.succeeded := true; v.state_changed := true; v.entity_type := 'Order'; \
                       v.entity_id := n; v.entity := jsonb_build_object('id', n); RETURN v;";

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
        format!("CREATE TABLE {SCHEMA}.tb_call (via text NOT NULL, args jsonb NOT NULL)"),
        format!(
            "CREATE VIEW {SCHEMA}.v_order AS SELECT NULL::uuid AS id, \
             jsonb_build_object('id', NULL::uuid) AS data WHERE false"
        ),
        format!(
            "CREATE FUNCTION {SCHEMA}.fn_create_order(p_buyer text, p_currency text, \
             p_rate float8, p_note text, p_line jsonb) RETURNS app.mutation_response \
             LANGUAGE plpgsql AS $$ DECLARE v app.mutation_response; n uuid := \
             gen_random_uuid(); BEGIN INSERT INTO {SCHEMA}.tb_call VALUES ('flatten', \
             jsonb_build_array(p_buyer, p_currency, p_rate, p_note, p_line)); {RESPOND} \
             END; $$"
        ),
        format!(
            "CREATE FUNCTION {SCHEMA}.fn_record_order(p_input jsonb) RETURNS \
             app.mutation_response LANGUAGE plpgsql AS $$ DECLARE v app.mutation_response; \
             n uuid := gen_random_uuid(); BEGIN INSERT INTO {SCHEMA}.tb_call VALUES ('jsonb', \
             jsonb_build_array(p_input)); {RESPOND} END; $$"
        ),
        format!(
            "CREATE FUNCTION {SCHEMA}.fn_quote(p_input jsonb) RETURNS TABLE (data jsonb) \
             LANGUAGE sql STABLE AS $$ SELECT jsonb_build_object('received', p_input) $$"
        ),
    ];
    stmts.extend(fraiseql_test_support::changelog::entity_change_log_provision_statements());
    for stmt in stmts {
        adapter.execute_raw_query(&stmt).await.unwrap_or_else(|e| panic!("{stmt}: {e}"));
    }
}

/// `OrderInput { buyer: String!, currency: String = "EUR", rate: Float! = 0.2,
/// note: String = "none", line: LineInput }` and `LineInput { sku: String!, qty: Int! = 1 }`,
/// written by `createOrder` (flattened to positional arguments) and `recordOrder`
/// (`input_style = jsonb`, one `jsonb` argument).
fn schema() -> CompiledSchema {
    let mut schema = CompiledSchema::new();
    let mut order = TypeDefinition::new("Order", format!("{SCHEMA}.v_order"));
    order.fields = vec![FieldDefinition::nullable("id", FieldType::Id)];
    schema.types.push(order);
    // The REST resource the write route hangs off.
    schema.queries.push(
        QueryDefinition::new("orders", "Order")
            .returning_list()
            .with_sql_source(format!("{SCHEMA}.v_order")),
    );

    let mut line = InputObjectDefinition::new("LineInput");
    line.fields = vec![
        InputFieldDefinition::new("sku", "String!").with_nullable(false),
        InputFieldDefinition::new("qty", "Int!")
            .with_nullable(false)
            .with_default_value("1"),
    ];
    let mut input = InputObjectDefinition::new("OrderInput");
    input.fields = vec![
        InputFieldDefinition::new("buyer", "String!").with_nullable(false),
        InputFieldDefinition::new("currency", "String").with_default_value("\"EUR\""),
        InputFieldDefinition::new("rate", "Float!")
            .with_nullable(false)
            .with_default_value("0.2"),
        InputFieldDefinition::new("note", "String").with_default_value("\"none\""),
        InputFieldDefinition::new("line", "LineInput"),
    ];
    schema.input_types.extend([line, input]);

    // `quote(input: OrderInput!)` reads `fn_quote`, which answers with what it was bound.
    let mut quote_type = TypeDefinition::new("Quote", format!("{SCHEMA}.fn_quote"));
    quote_type.fields = vec![FieldDefinition::nullable("received", FieldType::Json)];
    schema.types.push(quote_type);
    let mut quote = QueryDefinition::new("quote", "Quote")
        .returning_list()
        .with_sql_source(format!("{SCHEMA}.fn_quote"));
    quote.sql_source_kind = SourceKind::Function;
    quote.arguments = vec![ArgumentDefinition::new(
        "input",
        FieldType::Object("OrderInput".to_string()),
    )];
    schema.queries.push(quote);

    let input_arg = || {
        vec![ArgumentDefinition::new(
            "input",
            FieldType::Object("OrderInput".to_string()),
        )]
    };
    let mut create = MutationDefinition::new("createOrder", "Order");
    create.sql_source = Some(format!("{SCHEMA}.fn_create_order"));
    create.operation = MutationOperation::Insert {
        table: "tb_order".to_string(),
    };
    create.arguments = input_arg();
    let mut record = MutationDefinition::new("recordOrder", "Order");
    record.sql_source = Some(format!("{SCHEMA}.fn_record_order"));
    record.operation = MutationOperation::Insert {
        table: "tb_order".to_string(),
    };
    record.input_style = InputStyle::Jsonb;
    // `POST /orders` is `createOrder`'s.
    record.rest_path = Some("/order-records".to_string());
    record.arguments = input_arg();
    schema.mutations.extend([create, record]);
    schema.rest_config = Some(RestConfig {
        enabled: true,
        ..RestConfig::default()
    });
    schema.build_indexes();
    schema
}

/// The arguments of every call the functions received, in order.
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

const CREATE: &str = "mutation($i: OrderInput!) { createOrder(input: $i) { id } }";
const RECORD: &str = "mutation($i: OrderInput!) { recordOrder(input: $i) { id } }";

/// What `fn_create_order` receives for `{buyer: "c"}`: every default.
fn defaulted() -> Value {
    json!({ "via": "flatten", "args": ["c", "EUR", 0.2, "none", null] })
}

#[tokio::test]
async fn an_omitted_field_reaches_the_function_as_its_default_through_a_variable() {
    let Some(url) = try_database_url() else {
        return;
    };
    let server = serve(&url).await;

    let body = graphql(&server, CREATE, &json!({ "i": { "buyer": "c" } })).await;

    assert!(body["errors"].is_null(), "{body}");
    assert_eq!(calls(&url).await, vec![defaulted()]);
}

#[tokio::test]
async fn an_omitted_field_reaches_the_function_as_its_default_from_a_literal() {
    let Some(url) = try_database_url() else {
        return;
    };
    let server = serve(&url).await;

    let body =
        graphql(&server, "mutation { createOrder(input: { buyer: \"c\" }) { id } }", &json!({}))
            .await;

    assert!(body["errors"].is_null(), "{body}");
    assert_eq!(calls(&url).await, vec![defaulted()]);
}

#[tokio::test]
async fn a_sent_value_and_a_sent_null_are_kept() {
    let Some(url) = try_database_url() else {
        return;
    };
    let server = serve(&url).await;

    let body = graphql(
        &server,
        CREATE,
        &json!({ "i": { "buyer": "c", "currency": "USD", "rate": 0.1, "note": null } }),
    )
    .await;

    assert!(body["errors"].is_null(), "{body}");
    assert_eq!(
        calls(&url).await,
        vec![json!({ "via": "flatten", "args": ["c", "USD", 0.1, null, null] })],
        "a null sent for a nullable field is null, not its default"
    );
}

#[tokio::test]
async fn a_null_sent_for_a_non_null_field_with_a_default_is_refused() {
    let Some(url) = try_database_url() else {
        return;
    };
    let server = serve(&url).await;

    let body = graphql(&server, CREATE, &json!({ "i": { "buyer": "c", "rate": null } })).await;

    let message = body["errors"][0]["message"].as_str().unwrap_or_else(|| panic!("{body}"));
    assert!(message.contains("rate"), "{message}");
    assert_eq!(calls(&url).await, Vec::<Value>::new(), "the function never ran");
}

#[tokio::test]
async fn a_nested_input_objects_omitted_field_takes_its_default() {
    let Some(url) = try_database_url() else {
        return;
    };
    let server = serve(&url).await;

    let body =
        graphql(&server, CREATE, &json!({ "i": { "buyer": "c", "line": { "sku": "s" } } })).await;

    assert!(body["errors"].is_null(), "{body}");
    assert_eq!(
        calls(&url).await,
        vec![json!({
            "via": "flatten",
            "args": ["c", "EUR", 0.2, "none", { "sku": "s", "qty": 1 }],
        })]
    );
}

#[tokio::test]
async fn a_jsonb_input_carries_the_defaults_and_leaves_an_undefaulted_field_absent() {
    let Some(url) = try_database_url() else {
        return;
    };
    let server = serve(&url).await;

    let body = graphql(&server, RECORD, &json!({ "i": { "buyer": "c" } })).await;

    assert!(body["errors"].is_null(), "{body}");
    assert_eq!(
        calls(&url).await,
        vec![json!({
            "via": "jsonb",
            "args": [{ "buyer": "c", "currency": "EUR", "rate": 0.2, "note": "none" }],
        })]
    );
}

#[tokio::test]
async fn a_rest_write_takes_the_defaults_too() {
    let Some(url) = try_database_url() else {
        return;
    };
    let server = serve(&url).await;

    let response = reqwest::Client::new()
        .post(format!("{}/rest/v1/orders", server.base))
        .json(&json!({ "input": { "buyer": "c" } }))
        .send()
        .await
        .unwrap();

    let status = response.status();
    assert!(status.is_success(), "{status} {}", response.text().await.unwrap());
    assert_eq!(calls(&url).await, vec![defaulted()]);
}

#[tokio::test]
async fn a_function_root_reads_with_the_defaults_bound() {
    let Some(url) = try_database_url() else {
        return;
    };
    let server = serve(&url).await;

    let body = graphql(
        &server,
        "query($i: OrderInput!) { quote(input: $i) { received } }",
        &json!({ "i": { "buyer": "c" } }),
    )
    .await;

    assert!(body["errors"].is_null(), "{body}");
    assert_eq!(
        body["data"]["quote"][0]["received"],
        json!({ "buyer": "c", "currency": "EUR", "rate": 0.2, "note": "none" })
    );
}
