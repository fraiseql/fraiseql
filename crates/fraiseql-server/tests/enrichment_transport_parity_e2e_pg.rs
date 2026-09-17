//! #1336: `[identity.enrichment]`'s contract is "when enrichment is enabled, *every*
//! authenticated request resolves and fail-closes". Only `/graphql` honours it.
//!
//! This suite drives the **same deployment** through three transports and asserts they
//! answer the same question the same way. (gRPC's producer is covered by
//! `routes::grpc::tests::principal_production` rather than here: reaching it end to end
//! needs a live JWKS endpoint, and the producer is the whole of what changed.) The GraphQL cases are the control: they pass
//! today, which is what makes the REST cases evidence of a transport gap rather than a
//! broken fixture. If a GraphQL case ever reddens, the fixture is wrong and the REST
//! verdicts mean nothing.
//!
//! Two independent halves, because they fail for different reasons and a suite that
//! asserted only one of them would pass while the other stayed open:
//!
//! * **the refusal half** — a route whose query reads *no* enriched field. An unknown
//!   subject must be refused (403) because the deployment enabled enrichment, not
//!   because the operation happened to need an enriched value. Today REST serves it.
//! * **the read half** — a route whose query injects an enriched field. A *known*
//!   subject must get its own rows. Today REST errors for every caller, because
//!   `resolve_session_variables` hard-fails the `Enrichment` arm when nothing resolved.
//!
//! The token carries **only** `sub`; `org_id` exists in the actor table and nowhere in
//! the JWT. So a row set scoped to `tenant-a` can only have come from the database
//! resolve — a raw-claim fallback would produce no rows at all.
//!
//! Self-skips when no `DATABASE_URL` is set (no `#[ignore]`), so it is inert in the
//! database-free `test` leg and runs in the Dagger `integration: server` suite.
//!
//! **Execution engine:** `PostgreSQL` · **Infrastructure:** `DATABASE_URL` ·
//! **Parallelism:** creates and drops its own `tf_p36_*` fixtures and sets a
//! process-global env var for the HS256 secret → run `--test-threads=1`.
#![cfg(all(feature = "rest", feature = "mcp"))]
#![allow(clippy::unwrap_used, clippy::panic, clippy::print_stderr)] // Reason: test code — panics and skip diagnostics are acceptable

use std::sync::Arc;

use fraiseql_core::{
    db::postgres::PostgresAdapter,
    prelude::DatabaseAdapter as _,
    runtime::Executor,
    schema::{CompiledSchema, FieldType, InjectedParamSource, McpConfig, RestConfig},
    security::{AuthConfig, AuthMiddleware},
};
use fraiseql_server::{
    Server,
    identity::{EnrichmentQueryConfig, IdentityResolver},
    mcp::handler::{FraiseQLMcpService, McpTokenValidator},
    routes::graphql::AppState,
    server_config::{Hs256Config, ServerConfig},
};
use fraiseql_test_support::try_database_url;
use fraiseql_test_utils::schema_builder::{
    TestFieldBuilder, TestQueryBuilder, TestSchemaBuilder, TestTypeBuilder,
};
use serde_json::{Value, json};

/// The actor table the enrichment query resolves against.
const ACTOR_TABLE: &str = "tf_p36_actor";
/// The order fixture and the view the enriched query reads.
const ORDER_TABLE: &str = "tf_p36_order";
const ORDER_VIEW: &str = "v_p36_order";
/// The view the *unenriched* query reads — same rows, no enriched predicate.
const AUDIT_VIEW: &str = "v_p36_audit";

/// 32-byte HS256 secret — meets the minimum key-length requirement.
const SECRET: &str = "fraiseql-p36-secret-exactly-32by";
const SECRET_ENV: &str = "FRAISEQL_P36_HS256_SECRET";
const ISSUER: &str = "https://p36.fraiseql.test";
const AUDIENCE: &str = "fraiseql-p36-api";

/// The subject the actor table knows, and the org the *database* says it belongs to.
const KNOWN_SUB: &str = "p36-known-subject";
const KNOWN_ORG: &str = "tenant-a";
/// A subject that authenticates (the token is validly signed) but that the actor
/// table has never heard of. This is the one the contract says must be refused.
const UNKNOWN_SUB: &str = "p36-unknown-subject";

/// Rows per tenant, deliberately distinct so "saw everyone's rows" and "saw the wrong
/// tenant's rows" both fail on length alone.
const ROWS_A: usize = 2;
const ROWS_B: usize = 3;

// ---------------------------------------------------------------------------
// Fixture
// ---------------------------------------------------------------------------

async fn seed(adapter: &PostgresAdapter) {
    let mut stmts = vec![
        format!("DROP VIEW IF EXISTS {ORDER_VIEW}"),
        format!("DROP VIEW IF EXISTS {AUDIT_VIEW}"),
        format!("DROP TABLE IF EXISTS {ORDER_TABLE}"),
        format!("DROP TABLE IF EXISTS {ACTOR_TABLE}"),
        format!("CREATE TABLE {ACTOR_TABLE} (sub text PRIMARY KEY, org_id text NOT NULL)"),
        format!("INSERT INTO {ACTOR_TABLE} VALUES ('{KNOWN_SUB}', '{KNOWN_ORG}')"),
        format!("CREATE TABLE {ORDER_TABLE} (id bigint PRIMARY KEY, data jsonb NOT NULL)"),
    ];

    let mut id = 1;
    for (tenant, count) in [(KNOWN_ORG, ROWS_A), ("tenant-b", ROWS_B)] {
        for n in 1..=count {
            let data = json!({"id": id, "tenant_id": tenant, "label": format!("{tenant}-{n}")});
            stmts.push(format!("INSERT INTO {ORDER_TABLE} VALUES ({id}, '{data}'::jsonb)"));
            id += 1;
        }
    }
    stmts.push(format!("CREATE VIEW {ORDER_VIEW} AS SELECT id, data FROM {ORDER_TABLE}"));
    stmts.push(format!("CREATE VIEW {AUDIT_VIEW} AS SELECT id, data FROM {ORDER_TABLE}"));

    for stmt in stmts {
        let _: Vec<std::collections::HashMap<String, Value>> =
            adapter.execute_raw_query(&stmt).await.expect("fixture setup");
    }
}

/// Two resources, because REST derives one route group per **return type**:
///
/// * `P36Order` / `orders` — injects `enrichment:org_id`, so it *reads* the resolved identity;
/// * `P36Audit` / `audits` — injects nothing, so it reads no enriched field at all.
///
/// The second is the one that makes the refusal half meaningful: it answers 200 whether
/// or not anything resolved, so only an explicit fail-closed gate can refuse it.
fn build_schema() -> CompiledSchema {
    let mut orders = TestQueryBuilder::new("orders", "P36Order")
        .returns_list(true)
        .with_sql_source(ORDER_VIEW)
        .build();
    orders
        .inject_params
        .insert("tenant_id".to_string(), InjectedParamSource::Enrichment("org_id".to_string()));

    let audits = TestQueryBuilder::new("audits", "P36Audit")
        .returns_list(true)
        .with_sql_source(AUDIT_VIEW)
        .build();

    let mut schema = TestSchemaBuilder::new()
        .with_type(
            TestTypeBuilder::new("P36Order", ORDER_VIEW)
                .with_field(TestFieldBuilder::new("id", FieldType::Int).build())
                .with_field(TestFieldBuilder::new("tenant_id", FieldType::String).build())
                .with_field(TestFieldBuilder::new("label", FieldType::String).build())
                .build(),
        )
        .with_type(
            TestTypeBuilder::new("P36Audit", AUDIT_VIEW)
                .with_field(TestFieldBuilder::new("id", FieldType::Int).build())
                .with_field(TestFieldBuilder::new("tenant_id", FieldType::String).build())
                .with_field(TestFieldBuilder::new("label", FieldType::String).build())
                .build(),
        )
        .with_query(orders)
        .with_query(audits)
        .build();

    schema.rest_config = Some(RestConfig {
        enabled: true,
        require_auth: true,
        ..RestConfig::default()
    });
    schema.build_indexes();
    schema
}

/// HS256 auth plus `[identity.enrichment]`, wired the way an operator would.
///
/// `identity` is populated through serde rather than a struct literal: the field's type
/// lives behind a `pub(crate)` re-export, so an integration test cannot name it — but it
/// can let inference do so.
fn server_config() -> ServerConfig {
    let mut config = ServerConfig {
        auth_hs256: Some(Hs256Config {
            secret_env: SECRET_ENV.to_string(),
            issuer:     Some(ISSUER.to_string()),
            audience:   Some(AUDIENCE.to_string()),
        }),
        // #874: production validate() refuses cors_enabled = true with no origins.
        cors_enabled: false,
        ..ServerConfig::default()
    };
    config.identity = serde_json::from_value(json!({
        "enrichment": {
            "enabled": true,
            "query": format!("SELECT org_id FROM {ACTOR_TABLE} WHERE sub = $sub"),
            "map": {"org_id": "org_id"},
        }
    }))
    .expect("[identity.enrichment] config");
    config
}

/// Mint an HS256 token carrying **only** `sub` — no `org_id`.
///
/// The absent `org_id` is load-bearing: the enriched predicate can only be satisfied
/// from the actor table, so a row set scoped to `tenant-a` proves a database resolve
/// happened rather than a claim being read.
fn token_for(sub: &str) -> String {
    use jsonwebtoken::{Algorithm, EncodingKey, Header, encode};

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("system time before epoch")
        .as_secs();

    encode(
        &Header::new(Algorithm::HS256),
        &json!({
            "sub": sub,
            "iss": ISSUER,
            "aud": AUDIENCE,
            "iat": now,
            "exp": now + 3600,
        }),
        &EncodingKey::from_secret(SECRET.as_bytes()),
    )
    .expect("mint HS256 token")
}

// ---------------------------------------------------------------------------
// Harness
// ---------------------------------------------------------------------------

struct Answer {
    status: u16,
    body:   Value,
}

impl Answer {
    /// Every row's `tenant_id`, for a REST envelope.
    fn rest_tenants(&self) -> Vec<String> {
        self.body
            .get("data")
            .and_then(Value::as_array)
            .map(|rows| {
                rows.iter()
                    .filter_map(|r| r.get("tenant_id"))
                    .filter_map(|v| v.as_str().map(ToString::to_string))
                    .collect()
            })
            .unwrap_or_default()
    }
}

async fn rest_get(base: &str, resource: &str, token: &str) -> Answer {
    let response = reqwest::Client::new()
        .get(format!("{base}/rest/v1/{resource}"))
        .header("authorization", format!("Bearer {token}"))
        .send()
        .await
        .expect("REST GET");
    let status = response.status().as_u16();
    let text = response.text().await.expect("response body");
    Answer {
        status,
        body: serde_json::from_str(&text).unwrap_or_else(|_| json!({ "raw": text })),
    }
}

async fn graphql_post(base: &str, query: &str, token: &str) -> Answer {
    let response = reqwest::Client::new()
        .post(format!("{base}/graphql"))
        .header("authorization", format!("Bearer {token}"))
        .json(&json!({ "query": query }))
        .send()
        .await
        .expect("GraphQL POST");
    let status = response.status().as_u16();
    let text = response.text().await.expect("response body");
    Answer {
        status,
        body: serde_json::from_str(&text).unwrap_or_else(|_| json!({ "raw": text })),
    }
}

/// Stand the fixture and a real `Server::serve_on_listener` mount up.
async fn start() -> Option<(String, tokio::sync::oneshot::Sender<()>)> {
    let url = try_database_url()?;
    let adapter = Arc::new(PostgresAdapter::new(&url).await.expect("connect to the test database"));
    seed(&adapter).await;

    // Safe under the runner: this suite is the only reader of the name, and it is read
    // at boot, before the server starts serving.
    std::env::set_var(SECRET_ENV, SECRET);

    let mut config = server_config();
    config.database_url = url;

    // The pool is what makes `[identity.enrichment]` live: `enrichment_pool` is this
    // argument, and with `None` the resolver is never constructed — the server logs
    // "enrichment cannot run" and serves every subject. That is a fixture failure mode
    // the control cases below exist to catch.
    let pool = sqlx::PgPool::connect(&config.database_url).await.expect("enrichment pool");

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let port = listener.local_addr().expect("local addr").port();
    let server = Box::pin(Server::new(config, build_schema(), adapter, Some(pool)))
        .await
        .expect("Server::new with [auth_hs256] and [identity.enrichment]");

    let (tx, rx) = tokio::sync::oneshot::channel::<()>();
    tokio::spawn(async move {
        server
            .serve_on_listener(listener, async {
                let _ = rx.await;
            })
            .await
    });
    tokio::time::sleep(std::time::Duration::from_millis(300)).await;
    Some((format!("http://127.0.0.1:{port}"), tx))
}

// ---------------------------------------------------------------------------
// The control: /graphql already honours the contract
// ---------------------------------------------------------------------------

#[tokio::test]
async fn graphql_refuses_a_subject_the_actor_table_does_not_know() {
    let Some((base, shutdown)) = start().await else {
        eprintln!("skipping #1336 enrichment parity: DATABASE_URL not set");
        return;
    };

    let answer = graphql_post(&base, "{ audits { id } }", &token_for(UNKNOWN_SUB)).await;

    assert_eq!(
        answer.status, 403,
        "control: /graphql already fail-closes an unresolved subject on an operation that \
         reads no enriched field. If this reddens the fixture is wrong — the resolver is not \
         wired, or the actor table is not the one it queries — and every REST verdict in this \
         file is meaningless. Body: {}",
        answer.body
    );

    let _ = shutdown.send(());
}

#[tokio::test]
async fn graphql_serves_the_resolved_subject_its_own_rows() {
    let Some((base, shutdown)) = start().await else {
        eprintln!("skipping #1336 enrichment parity: DATABASE_URL not set");
        return;
    };

    let answer =
        graphql_post(&base, "{ orders { id tenant_id } }", &token_for(KNOWN_SUB)).await;

    assert_eq!(
        answer.status, 200,
        "control: a known subject reading an enrichment-injected query must succeed on \
         /graphql. Body: {}",
        answer.body
    );
    let rows = answer
        .body
        .get("data")
        .and_then(|d| d.get("orders"))
        .and_then(Value::as_array)
        .map(Vec::len);
    assert_eq!(
        rows,
        Some(ROWS_A),
        "control: the enriched predicate must scope to the org the *database* holds for \
         this subject ({KNOWN_ORG}, {ROWS_A} rows). The token carries no org_id, so any \
         other count means the predicate did not come from the resolve. Body: {}",
        answer.body
    );

    let _ = shutdown.send(());
}

// ---------------------------------------------------------------------------
// The gap: REST runs the same deployment and answers differently
// ---------------------------------------------------------------------------

#[tokio::test]
async fn rest_refuses_a_subject_the_actor_table_does_not_know() {
    let Some((base, shutdown)) = start().await else {
        eprintln!("skipping #1336 enrichment parity: DATABASE_URL not set");
        return;
    };

    let answer = rest_get(&base, "audits", &token_for(UNKNOWN_SUB)).await;

    assert_eq!(
        answer.status, 403,
        "#1336: with [identity.enrichment] enabled, *every* authenticated request resolves \
         and fail-closes — not only those that read an enriched field. This route reads \
         none, which is exactly why it is the discriminating case: it answers 200 whether \
         or not anything resolved. /graphql refuses this token; REST served it. Body: {}",
        answer.body
    );

    let _ = shutdown.send(());
}

#[tokio::test]
async fn rest_admits_a_subject_the_actor_table_knows() {
    let Some((base, shutdown)) = start().await else {
        eprintln!("skipping #1336 enrichment parity: DATABASE_URL not set");
        return;
    };

    let answer = rest_get(&base, "audits", &token_for(KNOWN_SUB)).await;

    assert_eq!(
        answer.status, 200,
        "the positive twin: fail-closed must refuse the *unresolved* subject, not every \
         subject. With this case absent, a gate that denied unconditionally would pass the \
         refusal test above. Body: {}",
        answer.body
    );

    let _ = shutdown.send(());
}

#[tokio::test]
async fn rest_serves_the_resolved_subject_its_own_rows() {
    let Some((base, shutdown)) = start().await else {
        eprintln!("skipping #1336 enrichment parity: DATABASE_URL not set");
        return;
    };

    let answer = rest_get(&base, "orders", &token_for(KNOWN_SUB)).await;

    assert_eq!(
        answer.status, 200,
        "#1336: a REST read over a query injecting an enriched field fails for *every* \
         caller today — `resolve_session_variables` hard-fails the Enrichment arm because \
         nothing resolved. Enriched-identity scoping is therefore unusable over REST. \
         Body: {}",
        answer.body
    );
    assert_eq!(
        answer.rest_tenants(),
        vec![KNOWN_ORG.to_string(); ROWS_A],
        "the read half: the rows must be scoped to the org the database holds for this \
         subject. The token carries no org_id, so this predicate can only have come from \
         the resolve. Body: {}",
        answer.body
    );

    let _ = shutdown.send(());
}

// ---------------------------------------------------------------------------
// MCP: the same deployment, the third door
// ---------------------------------------------------------------------------

/// The enrichment profile the server builds its resolver from, built here directly so
/// the MCP cases configure the same resolve the HTTP cases get through `ServerConfig`.
fn enrichment_config() -> EnrichmentQueryConfig {
    serde_json::from_value(json!({
        "enabled": true,
        "query": format!("SELECT org_id FROM {ACTOR_TABLE} WHERE sub = $sub"),
        "map": {"org_id": "org_id"},
    }))
    .expect("[identity.enrichment] profile")
}

/// An MCP service over the same schema, actor table and HS256 secret as the HTTP
/// cases — driven through `call_tool_authenticated`, the documented testable seam
/// under `ServerHandler::call_tool`.
async fn mcp_service() -> Option<FraiseQLMcpService<PostgresAdapter>> {
    let url = try_database_url()?;
    let adapter = PostgresAdapter::new(&url).await.expect("connect to the test database");
    seed(&adapter).await;

    let pool = sqlx::PgPool::connect(&url).await.expect("enrichment pool");
    let state = AppState::new(Arc::new(Executor::new(build_schema(), Arc::new(adapter))))
        .with_identity_resolver(Arc::new(IdentityResolver::postgres(enrichment_config(), pool)));

    Some(
        FraiseQLMcpService::new(
            state,
            McpConfig {
                enabled: true,
                require_auth: true,
                ..McpConfig::default()
            },
        )
        .with_token_validator(Some(McpTokenValidator::Hs256(Arc::new(
            AuthMiddleware::from_config(AuthConfig {
                // The same issuer/audience the HTTP cases' `[auth_hs256]` block
                // declares: one token shape for all three transports. Without these the
                // validator rejects the token's `aud` outright and every MCP case
                // "passes" its refusal assertion for the wrong reason — which is what
                // the positive twin below exists to catch.
                issuer: Some(ISSUER.to_string()),
                audience: Some(AUDIENCE.to_string()),
                ..AuthConfig::with_hs256(SECRET)
            }),
        )))),
    )
}

async fn mcp_call(
    service: &FraiseQLMcpService<PostgresAdapter>,
    tool: &str,
    sub: &str,
) -> rmcp::model::CallToolResult {
    service
        .call_tool_authenticated(
            tool,
            None,
            Some(token_for(sub)),
            format!("mcp-p36-{sub}"),
            &axum::http::HeaderMap::new(),
        )
        .await
}

#[tokio::test]
async fn mcp_refuses_a_subject_the_actor_table_does_not_know() {
    let Some(service) = mcp_service().await else {
        eprintln!("skipping #1336 enrichment parity: DATABASE_URL not set");
        return;
    };

    let result = mcp_call(&service, "audits", UNKNOWN_SUB).await;

    assert_eq!(
        result.is_error,
        Some(true),
        "#1336: MCP built a SecurityContext and never resolved it, so an unknown \
         subject was served the rows /graphql refuses it. This tool reads no enriched \
         field, which is what makes it the discriminating case. Content: {:?}",
        result.content
    );
}

#[tokio::test]
async fn mcp_admits_a_subject_the_actor_table_knows() {
    let Some(service) = mcp_service().await else {
        eprintln!("skipping #1336 enrichment parity: DATABASE_URL not set");
        return;
    };

    let result = mcp_call(&service, "audits", KNOWN_SUB).await;

    assert_ne!(
        result.is_error,
        Some(true),
        "the positive twin: a gate that refused every MCP call would satisfy the \
         refusal case above. Content: {:?}",
        result.content
    );
}

#[tokio::test]
async fn mcp_serves_the_resolved_subject_its_own_rows() {
    let Some(service) = mcp_service().await else {
        eprintln!("skipping #1336 enrichment parity: DATABASE_URL not set");
        return;
    };

    let result = mcp_call(&service, "orders", KNOWN_SUB).await;

    assert_ne!(
        result.is_error,
        Some(true),
        "#1336: an MCP tool call over a query injecting an enriched field failed for \
         every caller — the transport reached the engine's authenticated dispatch, so \
         it consumed enriched fields correctly and simply never produced any. \
         Content: {:?}",
        result.content
    );
    let text = result
        .content
        .first()
        .and_then(|c| c.as_text().map(|t| t.text.clone()))
        .unwrap_or_default();
    assert!(
        text.contains(KNOWN_ORG) && !text.contains("tenant-b"),
        "the rows must be scoped to the org the database holds for this subject; the \
         token carries no org_id, so the predicate can only have come from the \
         resolve. Got: {text}"
    );
}

// ---------------------------------------------------------------------------
// The boot check that makes the engine's backstop sound
// ---------------------------------------------------------------------------

#[tokio::test]
async fn a_schema_that_reads_enriched_identity_refuses_to_boot_without_a_resolver() {
    let Some(url) = try_database_url() else {
        eprintln!("skipping #1336 enrichment parity: DATABASE_URL not set");
        return;
    };
    let adapter = Arc::new(PostgresAdapter::new(&url).await.expect("connect"));

    // The same schema the cases above serve — it injects `enrichment:org_id` — but a
    // deployment that never enabled `[identity.enrichment]`.
    let config = ServerConfig {
        cors_enabled: false,
        database_url: url,
        ..ServerConfig::default()
    };

    let outcome = Box::pin(Server::new(config, build_schema(), adapter, None)).await;

    let Err(error) = outcome else {
        panic!(
            "#1336: a schema whose reads are scoped by a DB-derived identity, in a \
             deployment that resolves none, must not boot. Every request reading an \
             enriched field answers \"enrichment did not run\", and every other request \
             is served without the fail-closed check the schema implies — a 100% failure \
             rate whose only previous symptom was in production."
        );
    };
    let message = error.to_string();
    assert!(
        message.contains("identity.enrichment"),
        "the refusal must name the config block an operator has to change, not just \
         fail: {message}"
    );
}

#[tokio::test]
async fn the_same_schema_boots_once_a_resolver_is_configured() {
    // The positive twin. Without it, a check that refused every boot would satisfy the
    // case above — and the seven cases before it never construct a `Server` at all.
    let Some(url) = try_database_url() else {
        eprintln!("skipping #1336 enrichment parity: DATABASE_URL not set");
        return;
    };
    let adapter = Arc::new(PostgresAdapter::new(&url).await.expect("connect"));
    std::env::set_var(SECRET_ENV, SECRET);

    let mut config = server_config();
    config.database_url = url.clone();
    let pool = sqlx::PgPool::connect(&url).await.expect("enrichment pool");

    Box::pin(Server::new(config, build_schema(), adapter, Some(pool)))
        .await
        .expect("the identical schema must boot when [identity.enrichment] is enabled");
}
