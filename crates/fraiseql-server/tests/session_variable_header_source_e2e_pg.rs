//! A `source = "header"` session variable is the value of the request's HTTP header (#1520).
//!
//! It used to read `SecurityContext.attributes`, whose only writer on the request path copies
//! **JWT claims**: a mapping of `app.region` to the `x-region` header resolved to a claim named
//! `x-region`, or to nothing. And a request without a principal got no session variables at
//! all, `literal` ones included. This suite boots a real server against PostgreSQL and reads
//! the variables back through a view that selects `current_setting(…, true)`, and through a
//! mutation function that records what it saw, on every transport that reaches the engine
//! with headers: GraphQL (reads, writes, `@stream` continuations, `_entities`), REST (reads
//! and writes), MCP (reads and writes) and async operations. gRPC is driven in
//! `locale_grpc_e2e_pg`, whose rig builds the service from descriptors.
//!
//! **Execution engine:** `PostgreSQL` · **Infrastructure:** `DATABASE_URL` ·
//! **Parallelism:** creates and drops its own `v_issue_header_var` view and
//! `header_var_write` schema, and sets a process-global env var for the HS256 secret → run
//! `--test-threads=1`.
#![cfg(all(feature = "rest", feature = "mcp", feature = "auth"))]
#![allow(clippy::unwrap_used, clippy::panic, clippy::print_stderr)] // Reason: test code — panics and skip diagnostics are acceptable

use std::sync::Arc;

use fraiseql_core::{
    db::postgres::PostgresAdapter,
    prelude::DatabaseAdapter as _,
    runtime::Executor,
    schema::{
        ArgumentDefinition, CompiledSchema, FieldDefinition, FieldType, McpConfig,
        MutationDefinition, MutationOperation, QueryDefinition, RestConfig, SessionVariableMapping,
        SessionVariableSource, SessionVariablesConfig, TypeDefinition,
    },
};
use fraiseql_server::{
    Server,
    mcp::handler::FraiseQLMcpService,
    routes::graphql::AppState,
    server_config::{AsyncOperationsConfig, Hs256Config, ServerConfig},
};
use fraiseql_test_support::try_database_url;
use fraiseql_test_utils::schema_builder::{
    TestFieldBuilder, TestQueryBuilder, TestSchemaBuilder, TestTypeBuilder,
};
use serde_json::{Value, json};

const VIEW: &str = "v_issue_header_var";
/// The schema the write fixture lives in.
const WRITE_SCHEMA: &str = "header_var_write";
const SECRET: &str = "fraiseql-hdrvar-secret-exactly-32";
const SECRET_ENV: &str = "FRAISEQL_HDRVAR_HS256_SECRET";
const ISSUER: &str = "https://hdrvar.fraiseql.test";
const AUDIENCE: &str = "fraiseql-hdrvar-api";

/// `app.region` from the `x-region` header, `app.flavor` a literal, `app.subject` the `sub`
/// claim.
fn session_variables() -> SessionVariablesConfig {
    SessionVariablesConfig {
        variables:         vec![
            SessionVariableMapping {
                name:   "app.region".to_string(),
                source: SessionVariableSource::Header {
                    header: "x-region".to_string(),
                },
            },
            SessionVariableMapping {
                name:   "app.flavor".to_string(),
                source: SessionVariableSource::Literal {
                    value: "vanilla".to_string(),
                },
            },
            SessionVariableMapping {
                name:   "app.subject".to_string(),
                source: SessionVariableSource::Jwt {
                    claim: "sub".to_string(),
                },
            },
        ],
        inject_started_at: false,
    }
}

/// `regionProbes` reads the three settings; `createRegionNote` calls a function that
/// records `app.region` as it saw it.
fn schema() -> CompiledSchema {
    let mut schema = TestSchemaBuilder::new()
        .with_type(
            TestTypeBuilder::new("RegionProbe", VIEW)
                .with_field(TestFieldBuilder::new("id", FieldType::Int).build())
                .with_field(TestFieldBuilder::nullable("region", FieldType::String).build())
                .with_field(TestFieldBuilder::nullable("flavor", FieldType::String).build())
                .with_field(TestFieldBuilder::nullable("subject", FieldType::String).build())
                .build(),
        )
        .with_query(
            TestQueryBuilder::new("regionProbes", "RegionProbe")
                .returns_list(true)
                .with_sql_source(VIEW)
                .build(),
        )
        // The same view as a relay connection, under a type of its own so REST's routing of
        // `RegionProbe` stays on `regionProbes`.
        .with_type(
            TestTypeBuilder::new("RelayProbe", VIEW)
                .with_field(TestFieldBuilder::new("id", FieldType::Int).build())
                .with_field(TestFieldBuilder::nullable("region", FieldType::String).build())
                .with_field(TestFieldBuilder::nullable("flavor", FieldType::String).build())
                .with_field(TestFieldBuilder::nullable("subject", FieldType::String).build())
                .build(),
        )
        .with_query(
            TestQueryBuilder::new("regionProbeConnection", "RelayProbe")
                .returns_list(true)
                .with_sql_source(VIEW)
                .relay_cursor_column("id")
                .build(),
        )
        .build();
    let mut note = TypeDefinition::new("RegionNote", format!("{WRITE_SCHEMA}.v_note"));
    note.fields = vec![
        FieldDefinition::new("id", FieldType::Id),
        FieldDefinition::new("label", FieldType::String),
    ];
    schema.types.push(note);
    schema.queries.push(
        QueryDefinition::new("notes", "RegionNote")
            .returning_list()
            .with_sql_source(format!("{WRITE_SCHEMA}.v_note")),
    );
    let mut by_id = QueryDefinition::new("note", "RegionNote")
        .with_sql_source(format!("{WRITE_SCHEMA}.v_note"));
    by_id.arguments = vec![ArgumentDefinition::new("id", FieldType::Id)];
    schema.queries.push(by_id);
    let mut create = MutationDefinition::new("createRegionNote", "RegionNote");
    create.sql_source = Some(format!("{WRITE_SCHEMA}.fn_create_note"));
    create.operation = MutationOperation::Insert {
        table: "tb_note".to_string(),
    };
    create.arguments = vec![ArgumentDefinition::new("label", FieldType::String)];
    schema.mutations.push(create);
    schema.rest_config = Some(RestConfig {
        enabled: true,
        ..RestConfig::default()
    });
    schema.session_variables = session_variables();
    schema.build_indexes();
    schema
}

/// The probe view, and a note table whose mutation function records `app.region`.
async fn provision(adapter: &PostgresAdapter) {
    let mut stmts = vec![
        format!("DROP VIEW IF EXISTS {VIEW}"),
        format!(
            "CREATE VIEW {VIEW} AS SELECT 1 AS id, jsonb_build_object('id', 1, \
             'region', current_setting('app.region', true), \
             'flavor', current_setting('app.flavor', true), \
             'subject', current_setting('app.subject', true)) AS data"
        ),
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
        format!("DROP SCHEMA IF EXISTS {WRITE_SCHEMA} CASCADE"),
        format!("CREATE SCHEMA {WRITE_SCHEMA}"),
        format!("CREATE TABLE {WRITE_SCHEMA}.tb_seen (label text, region text)"),
        format!("CREATE TABLE {WRITE_SCHEMA}.tb_note (id uuid PRIMARY KEY, label text NOT NULL)"),
        format!(
            "CREATE VIEW {WRITE_SCHEMA}.v_note AS SELECT id, jsonb_build_object('id', id, \
             'label', label) AS data FROM {WRITE_SCHEMA}.tb_note"
        ),
        format!(
            "CREATE FUNCTION {WRITE_SCHEMA}.fn_create_note(p_label text) \
             RETURNS app.mutation_response LANGUAGE plpgsql AS $$ \
             DECLARE v app.mutation_response; n uuid; BEGIN \
             INSERT INTO {WRITE_SCHEMA}.tb_seen VALUES (p_label, \
               current_setting('app.region', true)); \
             n := gen_random_uuid(); \
             INSERT INTO {WRITE_SCHEMA}.tb_note (id, label) VALUES (n, p_label); \
             v.succeeded := true; v.state_changed := true; v.message := 'created'; \
             v.entity_type := 'RegionNote'; v.entity_id := n; \
             v.entity := jsonb_build_object('id', n, 'label', p_label); \
             RETURN v; END; $$"
        ),
    ];
    stmts.extend(fraiseql_test_support::changelog::entity_change_log_provision_statements());
    for stmt in stmts {
        adapter.execute_raw_query(&stmt).await.unwrap_or_else(|e| panic!("{stmt}: {e}"));
    }
}

/// What the mutation function recorded as `app.region` for the write labelled `label`.
async fn seen(url: &str, label: &str) -> Option<String> {
    let adapter = PostgresAdapter::new(url).await.unwrap();
    let rows = adapter
        .execute_raw_query(&format!(
            "SELECT region FROM {WRITE_SCHEMA}.tb_seen WHERE label = '{label}'"
        ))
        .await
        .unwrap();
    assert_eq!(rows.len(), 1, "the function ran once for {label}: {rows:?}");
    rows[0]["region"].as_str().map(str::to_string)
}

fn token(extra: &Value) -> String {
    use jsonwebtoken::{Algorithm, EncodingKey, Header, encode};
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let mut claims =
        json!({ "sub": "probe", "iss": ISSUER, "aud": AUDIENCE, "iat": now, "exp": now + 3600 });
    if let (Some(claims), Some(extra)) = (claims.as_object_mut(), extra.as_object()) {
        claims.extend(extra.clone());
    }
    encode(
        &Header::new(Algorithm::HS256),
        &claims,
        &EncodingKey::from_secret(SECRET.as_bytes()),
    )
    .unwrap()
}

/// A running server; dropping it shuts the server down.
struct Running {
    base:      String,
    _shutdown: tokio::sync::oneshot::Sender<()>,
}

/// A server against `url` with HS256 authentication, or none (every request is anonymous).
async fn serve(url: &str, authenticated: bool) -> Running {
    serve_with(url, authenticated, |_| {}).await
}

async fn serve_with(
    url: &str,
    authenticated: bool,
    tweak: impl FnOnce(&mut ServerConfig),
) -> Running {
    let adapter = Arc::new(PostgresAdapter::new(url).await.unwrap());
    provision(&adapter).await;
    std::env::set_var(SECRET_ENV, SECRET);
    let mut config = ServerConfig {
        database_url: url.to_string(),
        auth_hs256: authenticated.then(|| Hs256Config {
            secret_env: SECRET_ENV.to_string(),
            issuer:     Some(ISSUER.to_string()),
            audience:   Some(AUDIENCE.to_string()),
        }),
        cors_enabled: false,
        ..ServerConfig::default()
    };
    tweak(&mut config);
    // The pool backs async operations; inert otherwise.
    let pool = Some(sqlx::PgPool::connect(url).await.unwrap());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    // REST writes are mounted only on request (#865).
    // As the server binary builds it: relay-capable (main.rs).
    let server = Box::pin(Server::with_relay_pagination(config, schema(), adapter, pool))
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

/// Whether a probed setting is unset. A pooled connection that once had a setting applied
/// transaction-locally reads it back as `''`, not `NULL`, once the transaction ends.
fn unset(value: &Value) -> bool {
    value.is_null() || value == &json!("")
}

/// A response body as JSON, or `{status, body}` when it is not JSON.
async fn body(response: reqwest::Response) -> Value {
    let status = response.status();
    let text = response.text().await.unwrap();
    serde_json::from_str(&text)
        .unwrap_or_else(|_| json!({ "status": status.as_u16(), "body": text }))
}

/// A GraphQL `POST` of `query`, with `token` (if any) and `headers`.
async fn graphql(
    server: &Running,
    query: &str,
    token: Option<String>,
    headers: &[(&str, &str)],
) -> Value {
    let mut request = reqwest::Client::new()
        .post(format!("{}/graphql", server.base))
        .json(&json!({ "query": query }));
    if let Some(token) = token {
        request = request.header("authorization", format!("Bearer {token}"));
    }
    for (name, value) in headers {
        request = request.header(*name, *value);
    }
    body(request.send().await.unwrap()).await
}

const PROBE: &str = "{ regionProbes { region flavor subject } }";

/// The request's header, never a claim of the same name; with or without a principal.
#[tokio::test]
async fn a_header_session_variable_reads_the_request_header() {
    let Some(url) = try_database_url() else {
        eprintln!("skipping: DATABASE_URL not set");
        return;
    };
    let server = serve(&url, true).await;
    let sent = graphql(&server, PROBE, Some(token(&json!({}))), &[("x-region", "eu")]).await;
    let claimed = graphql(&server, PROBE, Some(token(&json!({ "x-region": "claim" }))), &[]).await;
    drop(server);

    let server = serve(&url, false).await;
    let anonymous = graphql(&server, PROBE, None, &[("x-region", "eu")]).await;
    let twice = graphql(&server, PROBE, None, &[("x-region", "eu"), ("x-region", "us")]).await;
    drop(server);

    assert_eq!(
        sent["data"]["regionProbes"][0]["region"],
        json!("eu"),
        "app.region is the x-region header the request sent: {sent}"
    );
    assert!(
        unset(&claimed["data"]["regionProbes"][0]["region"]),
        "a claim named x-region is not the header, and no header was sent: {claimed}"
    );
    let row = &anonymous["data"]["regionProbes"][0];
    assert!(
        row["region"] == json!("eu") && row["flavor"] == json!("vanilla") && unset(&row["subject"]),
        "an anonymous request gets its header and literal variables, not the claim: {anonymous}"
    );
    assert!(
        twice["data"].is_null()
            && twice["errors"][0]["message"]
                .as_str()
                .is_some_and(|m| m.contains("more than once")),
        "a header sent twice is refused, not joined: {twice}"
    );
}

/// A relay connection reads with the same variables as a list, principal or not: an
/// anonymous page gets its header and literal ones.
#[tokio::test]
async fn a_relay_connection_sees_the_header_and_literal_variables() {
    const PAGE: &str =
        "{ regionProbeConnection(first: 1) { edges { node { region flavor subject } } } }";
    let Some(url) = try_database_url() else {
        eprintln!("skipping: DATABASE_URL not set");
        return;
    };
    let server = serve(&url, true).await;
    let signed = graphql(&server, PAGE, Some(token(&json!({}))), &[("x-region", "eu")]).await;
    drop(server);
    let server = serve(&url, false).await;
    let anonymous = graphql(&server, PAGE, None, &[("x-region", "eu")]).await;
    drop(server);

    for (who, response) in [("authenticated", &signed), ("anonymous", &anonymous)] {
        let node = &response["data"]["regionProbeConnection"]["edges"][0]["node"];
        assert!(
            node["region"] == json!("eu") && node["flavor"] == json!("vanilla"),
            "{who}: the page read with app.region and app.flavor set: {response}"
        );
    }
}

/// A GraphQL mutation runs its function with the header's value, principal or not.
#[tokio::test]
async fn a_graphql_mutation_sees_the_header() {
    let Some(url) = try_database_url() else {
        return;
    };
    let server = serve(&url, false).await;
    let result = graphql(
        &server,
        r#"mutation { createRegionNote(label: "via-graphql") { id } }"#,
        None,
        &[("x-region", "eu")],
    )
    .await;
    assert!(result.get("errors").is_none(), "{result}");
    assert_eq!(seen(&url, "via-graphql").await.as_deref(), Some("eu"), "{result}");
}

/// REST reads (resolved in the extractor every handler shares) and writes see the header.
#[tokio::test]
async fn rest_reads_and_writes_see_the_header() {
    let Some(url) = try_database_url() else {
        return;
    };
    let server = serve(&url, false).await;
    let client = reqwest::Client::new();
    let read = body(
        client
            .get(format!("{}/rest/v1/regionProbes", server.base))
            .header("x-region", "eu")
            .send()
            .await
            .unwrap(),
    )
    .await;
    assert_eq!(read["data"][0]["region"], json!("eu"), "REST GET: {read}");

    let written = body(
        client
            .post(format!("{}/rest/v1/notes", server.base))
            .header("x-region", "eu")
            .json(&json!({ "label": "via-rest" }))
            .send()
            .await
            .unwrap(),
    )
    .await;
    assert_eq!(seen(&url, "via-rest").await.as_deref(), Some("eu"), "REST POST: {written}");
}

/// An MCP tool call, read or write, sees the header. Driven through
/// `call_tool_authenticated`, the seam under `ServerHandler::call_tool`, with the headers the
/// HTTP transport hands it.
#[tokio::test]
async fn an_mcp_tool_call_sees_the_header() {
    let Some(url) = try_database_url() else {
        return;
    };
    let adapter = Arc::new(PostgresAdapter::new(&url).await.unwrap());
    provision(&adapter).await;
    let service = FraiseQLMcpService::new(
        AppState::new(Arc::new(Executor::new(schema(), adapter))),
        McpConfig {
            enabled: true,
            require_auth: false,
            read_only: false,
            ..McpConfig::default()
        },
    );
    let mut headers = axum::http::HeaderMap::new();
    headers.insert("x-region", "eu".parse().unwrap());
    let text = |result: &rmcp::model::CallToolResult| {
        result
            .content
            .first()
            .and_then(|c| c.as_text().map(|t| t.text.clone()))
            .unwrap_or_default()
    };

    let read = service
        .call_tool_authenticated("regionProbes", None, None, "mcp-read".to_string(), &headers)
        .await;
    assert_ne!(read.is_error, Some(true), "{}", text(&read));
    assert!(text(&read).contains("\"eu\""), "the tool read saw the header: {}", text(&read));

    let arguments = json!({ "label": "via-mcp" });
    let written = service
        .call_tool_authenticated(
            "createRegionNote",
            arguments.as_object(),
            None,
            "mcp-write".to_string(),
            &headers,
        )
        .await;
    assert_ne!(written.is_error, Some(true), "{}", text(&written));
    assert_eq!(seen(&url, "via-mcp").await.as_deref(), Some("eu"), "MCP mutation");
}

/// A GraphQL `@stream` over SSE runs its continuation batches after the handler has
/// returned, where no scope reaches. `initialCount: 0` puts the probe row in a continuation.
#[tokio::test]
async fn a_streamed_continuation_batch_sees_the_header() {
    let Some(url) = try_database_url() else {
        return;
    };
    let server = serve_with(&url, false, |config| {
        config.enable_graphql_incremental = true;
        config.graphql_incremental_batch_size = Some(1);
    })
    .await;
    let text = reqwest::Client::new()
        .post(format!("{}/graphql", server.base))
        .header("accept", "text/event-stream")
        .header("x-region", "eu")
        .json(&json!({ "query": "{ regionProbes @stream(initialCount: 0) { region } }" }))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    let streamed: Vec<Value> = text
        .split("\n\n")
        .filter_map(|block| block.lines().find_map(|l| l.strip_prefix("data:")))
        .filter_map(|data| serde_json::from_str::<Value>(data.trim()).ok())
        .flat_map(|payload| {
            payload["incremental"].as_array().cloned().unwrap_or_default().into_iter()
        })
        .flat_map(|entry| entry["items"].as_array().cloned().unwrap_or_default().into_iter())
        .collect();
    assert_eq!(streamed, vec![json!({ "region": "eu" })], "the continuation batch: {text}");
}

/// An async operation executes later, on a worker with no request to read a header from.
/// The header is read at submission and stored with the operation.
#[tokio::test]
async fn an_async_operation_executes_with_the_header_it_was_submitted_with() {
    let Some(url) = try_database_url() else {
        return;
    };
    let server = serve_with(&url, true, |config| {
        config.async_operations = Some(AsyncOperationsConfig {
            operations: vec!["regionProbes".to_string()],
            workers: 1,
            poll_interval_ms: 100,
            ..AsyncOperationsConfig::default()
        });
    })
    .await;
    let client = reqwest::Client::new();
    let token = token(&json!({ "sub": "async-region" }));
    let submitted = body(
        client
            .post(format!("{}/operations/v1/regionProbes", server.base))
            .bearer_auth(&token)
            .header("x-region", "eu")
            .json(&json!({ "query": "{ regionProbes { region } }" }))
            .send()
            .await
            .unwrap(),
    )
    .await;
    let op_id = submitted["op_id"].as_str().unwrap_or_else(|| panic!("{submitted}")).to_string();
    let mut terminal = Value::Null;
    for _ in 0..100 {
        terminal = body(
            client
                .get(format!("{}/operations/v1/{op_id}", server.base))
                .bearer_auth(&token)
                .send()
                .await
                .unwrap(),
        )
        .await;
        if terminal["status"] != json!("queued") && terminal["status"] != json!("running") {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
    assert_eq!(
        terminal["result"]["data"]["regionProbes"][0]["region"],
        json!("eu"),
        "executed with the submission's header: {terminal}"
    );
}

/// A federation `_entities` lookup is a read through its own call of the session builder.
#[cfg(feature = "federation")]
#[tokio::test]
async fn a_federation_entity_lookup_sees_the_header() {
    let Some(url) = try_database_url() else {
        return;
    };
    let adapter = Arc::new(PostgresAdapter::new(&url).await.unwrap());
    for ddl in [
        "DROP VIEW IF EXISTS v_header_var_fed_probe",
        "CREATE VIEW v_header_var_fed_probe AS SELECT 'p1'::text AS id, jsonb_build_object('id', \
         'p1', 'region', current_setting('app.region', true)) AS data",
    ] {
        adapter.execute_raw_query(ddl).await.unwrap();
    }
    let mut schema = CompiledSchema::from_json(
        &json!({
            "fraiseql_version": env!("CARGO_PKG_VERSION"),
            "types": [{
                "name": "RegionFedProbe",
                "sql_source": "v_header_var_fed_probe",
                "fields": [
                    {"name": "id", "field_type": "ID", "nullable": false},
                    {"name": "region", "field_type": "String", "nullable": true}
                ]
            }],
            "queries": [{
                "name": "regionFedProbe", "return_type": "RegionFedProbe", "returns_list": false,
                "nullable": true, "sql_source": "v_header_var_fed_probe", "jsonb_column": "data",
                "arguments": [{"name": "id", "arg_type": "ID", "nullable": false}]
            }],
            "mutations": [], "subscriptions": [],
            "federation": {
                "enabled": true, "version": "v2", "service_name": "region",
                "entities": [{"name": "RegionFedProbe", "key_fields": ["id"]}]
            }
        })
        .to_string(),
        false,
    )
    .unwrap();
    schema.session_variables = session_variables();
    schema.build_indexes();
    let config = ServerConfig {
        database_url: url,
        cors_enabled: false,
        ..ServerConfig::default()
    };
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let server = Box::pin(Server::new(config, schema, adapter, None)).await.unwrap();
    let (_tx, rx) = tokio::sync::oneshot::channel::<()>();
    tokio::spawn(async move {
        server
            .serve_on_listener(listener, async {
                let _ = rx.await;
            })
            .await
    });
    tokio::time::sleep(std::time::Duration::from_millis(300)).await;
    let result = body(
        reqwest::Client::new()
            .post(format!("http://127.0.0.1:{port}/graphql"))
            .header("x-region", "eu")
            .json(&json!({
                "query": "query($representations: [_Any!]!) { _entities(representations: \
                          $representations) { ... on RegionFedProbe { id region } } }",
                "variables": {
                    "representations": [{ "__typename": "RegionFedProbe", "id": "p1" }]
                }
            }))
            .send()
            .await
            .unwrap(),
    )
    .await;
    assert_eq!(result["data"]["_entities"][0]["region"], json!("eu"), "{result}");
}

/// The suite's schemas load with no database.
#[test]
fn the_document_loads_without_a_database() {
    let schema = schema();
    CompiledSchema::from_json(&serde_json::to_string(&schema).unwrap(), false)
        .unwrap_or_else(|e| panic!("the header-variable suite's schema must load: {e}"));
}
