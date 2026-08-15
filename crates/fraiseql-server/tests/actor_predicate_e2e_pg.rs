//! #966 end to end: `requires_actor` is enforced **at execution, on every
//! transport**, and per-actor cost budgets are keyed on `(tenant, actor_type)`.
//!
//! The predicate itself is a slice check, pinned in
//! `security/actor_type/tests.rs`. What only this shape can observe is the claim
//! the issue actually makes — that no door around it exists. #808's lesson is
//! that a policy advertised in one transport and enforced in another is not
//! enforced, so this drives **four** of them at the same restricted operation and
//! requires the same refusal from each:
//!
//! | Door | Test |
//! |---|---|
//! | `POST /graphql` query | `graphql_refuses_a_restricted_query_to_an_agent` |
//! | `POST /graphql` mutation | `graphql_refuses_a_restricted_mutation_to_an_agent` |
//! | `GET /rest/v1/…` | `rest_refuses_the_same_restricted_query` |
//! | MCP `tools/call` | `mcp_refuses_the_same_restricted_query` |
//!
//! The relay `node(id:)` door — the fifth gate, and the one #1030 showed can be
//! forgotten — is proven at the executor level in `fraiseql-core`
//! (`tests/integration/relay_integration.rs`), because it needs a relay-capable
//! adapter this harness does not build.
//!
//! That set is not arbitrary: those are the paths that reach the database
//! through the executor's operation gates. A fifth transport added later inherits
//! the predicate for the same reason — the gate is inside the executor, not
//! beside each mount.
//!
//! **The delegation case is the point of the feature.** A delegated token (RFC
//! 8693 `act`) carries the *human's* roles, so `requires_role` admits it. The
//! agent must still be refused, or "agents cannot do this regardless of the
//! underlying user's permissions" is not what was built —
//! `a_delegated_agent_holding_the_role_is_still_refused` is that test, and it is
//! the one an implementation that consulted `acting_for` would fail alone.
//!
//! Self-skips when no `DATABASE_URL` is set (no `#[ignore]`), so it is inert in
//! the database-free `test` leg and runs in the Dagger `integration: server`
//! suite.
//!
//! **Execution engine:** `PostgreSQL` · **Infrastructure:** `DATABASE_URL` ·
//! **Parallelism:** recreates its own `p20_actor` schema → run
//! `--test-threads=1`.
#![allow(clippy::unwrap_used, clippy::panic, clippy::print_stderr)] // Reason: test code — panics and skip diagnostics are acceptable
#![allow(clippy::future_not_send)] // Reason: temp_env's env-guard makes boot() non-Send; each test runs it on its own tokio runtime

use std::sync::Arc;

use fraiseql_core::{
    db::postgres::PostgresAdapter,
    prelude::DatabaseAdapter as _,
    schema::{
        ArgumentDefinition, CompiledSchema, FieldDefinition, FieldType, MutationDefinition,
        MutationOperation, QueryDefinition, RestConfig, TypeDefinition,
    },
    security::ActorType,
};
use fraiseql_server::server_config::{Hs256Config, ServerConfig};
use fraiseql_test_support::try_database_url;
use serde_json::{Value, json};

mod common;

use crate::common::server_harness::TestServer;

const SCHEMA: &str = "p20_actor";
const SECRET_ENV: &str = "FRAISEQL_TEST_P20_ACTOR_HS256_SECRET";
const SECRET: &str = "p20-actor-secret-0123456789-0123456789";
const ISSUER: &str = "https://actorpred.test.fraiseql";
const AUDIENCE: &str = "actorpred-test";

/// UUID-shaped so a delegated token's `acting_for` resolves.
const HUMAN_SUB: &str = "5a1e0000-0000-4000-8000-000000000966";

fn database_url_or_skip(test: &str) -> Option<String> {
    let url = try_database_url();
    if url.is_none() {
        eprintln!("SKIP {test}: DATABASE_URL not set");
    }
    url
}

// ── Fixture ──────────────────────────────────────────────────────────────────

async fn provision(adapter: &PostgresAdapter) {
    let stmts = vec![
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
        format!(
            "CREATE TABLE {SCHEMA}.tb_secret (
               pk_secret bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
               id uuid NOT NULL UNIQUE DEFAULT gen_random_uuid(),
               label text NOT NULL
             )"
        ),
        format!("INSERT INTO {SCHEMA}.tb_secret (label) VALUES ('classified'), ('restricted')"),
        format!(
            "CREATE VIEW {SCHEMA}.v_secret AS
               SELECT pk_secret, id,
                      jsonb_build_object('id', id, 'label', label) AS data
               FROM {SCHEMA}.tb_secret ORDER BY pk_secret"
        ),
        format!(
            "CREATE FUNCTION {SCHEMA}.fn_create_secret(p_label text) \
             RETURNS app.mutation_response LANGUAGE plpgsql AS $$ \
             DECLARE v app.mutation_response; n uuid; BEGIN \
             INSERT INTO {SCHEMA}.tb_secret (label) VALUES (p_label) RETURNING id INTO n; \
             v.succeeded := true; v.state_changed := true; v.message := 'created'; \
             v.entity_type := 'Secret'; v.entity_id := n; \
             v.entity := jsonb_build_object('id', n, 'label', p_label); \
             RETURN v; END; $$"
        ),
    ];
    for stmt in stmts {
        let _: Vec<std::collections::HashMap<String, Value>> =
            adapter.execute_raw_query(&stmt).await.expect("fixture setup");
    }
}

/// Two operations restricted to humans, and one unrestricted control.
///
/// The control matters: without it, a server that refused *everything* would
/// pass every refusal test in this file.
fn schema() -> CompiledSchema {
    let mut schema = CompiledSchema::new();

    let mut secret = TypeDefinition::new("Secret", format!("{SCHEMA}.v_secret"));
    secret.fields = vec![
        FieldDefinition::new("id", FieldType::Id),
        FieldDefinition::new("label", FieldType::String),
    ];
    schema.types.push(secret);

    // The control: no restriction, reachable by everyone.
    schema.queries.push(
        QueryDefinition::new("openSecrets", "Secret")
            .returning_list()
            .with_sql_source(format!("{SCHEMA}.v_secret")),
    );

    // The restricted query gets its **own return type** over the same view.
    // REST derives one resource per return type and names it after that type's
    // first list query, so sharing `Secret` would have mounted only
    // `/rest/v1/openSecrets` — and the REST leg of this suite would have been
    // asserting a 404 against a route that never existed.
    let mut locked = TypeDefinition::new("LockedSecret", format!("{SCHEMA}.v_secret"));
    locked.fields = vec![
        FieldDefinition::new("id", FieldType::Id),
        FieldDefinition::new("label", FieldType::String),
    ];
    schema.types.push(locked);

    // Restricted to humans, and additionally role-gated so the ordering of the
    // two gates is observable (see `the_role_gate_still_runs_first`).
    let mut restricted = QueryDefinition::new("humanSecrets", "LockedSecret")
        .returning_list()
        .with_sql_source(format!("{SCHEMA}.v_secret"));
    restricted.requires_actor = vec![ActorType::HumanUser];
    restricted.requires_role = Some("reader".to_string());
    schema.queries.push(restricted);

    let mut create = MutationDefinition::new("createSecret", "Secret");
    create.sql_source = Some(format!("{SCHEMA}.fn_create_secret"));
    create.operation = MutationOperation::Insert {
        table: "tb_secret".to_string(),
    };
    create.arguments = vec![ArgumentDefinition::new("label", FieldType::String)];
    create.requires_actor = vec![ActorType::HumanUser];
    schema.mutations.push(create);

    schema.rest_config = Some(RestConfig {
        enabled: true,
        require_auth: true,
        ..RestConfig::default()
    });
    schema.build_indexes();
    schema
}

fn config() -> ServerConfig {
    ServerConfig {
        auth_hs256: Some(Hs256Config {
            secret_env: SECRET_ENV.to_string(),
            issuer:     Some(ISSUER.to_string()),
            audience:   Some(AUDIENCE.to_string()),
        }),
        cors_enabled: false,
        ..ServerConfig::default()
    }
}

/// Mint an HS256 token, merging `extra` over the base claims.
fn mint(sub: &str, extra: &Value) -> String {
    use jsonwebtoken::{Algorithm, EncodingKey, Header, encode};
    let now = i64::try_from(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system time")
            .as_secs(),
    )
    .expect("epoch seconds fit i64");
    let mut claims = json!({
        "sub": sub, "iss": ISSUER, "aud": AUDIENCE, "iat": now, "exp": now + 3600,
    });
    if let (Some(obj), Some(extra_obj)) = (claims.as_object_mut(), extra.as_object()) {
        for (k, v) in extra_obj {
            obj.insert(k.clone(), v.clone());
        }
    }
    encode(
        &Header::new(Algorithm::HS256),
        &claims,
        &EncodingKey::from_secret(SECRET.as_bytes()),
    )
    .expect("mint token")
}

/// An ordinary user token: no `act`, no `service_account` scope → `human_user`.
///
/// Carries the role as **both** a `roles` claim and a `scope`, because the two
/// `requires_role` gates on the path read different fields: the executor checks
/// `SecurityContext.roles`, the REST resolver checks `SecurityContext.scopes`
/// (`rest/handler/query.rs`). That divergence is pre-existing and out of this
/// issue's scope — filed as #1122 — but a token carrying only one of them is
/// refused by the *role* gate before the actor gate is reached, which would make
/// the REST leg of this suite assert nothing about `requires_actor`.
fn human_token() -> String {
    mint(HUMAN_SUB, &json!({ "roles": ["reader"], "scope": "reader" }))
}

/// A delegated token: the RFC 8693 `act` claim makes this an `ai_agent`, and it
/// carries the **human's** roles — which is exactly why `requires_role` cannot
/// express what `requires_actor` does.
fn agent_token() -> String {
    mint(
        HUMAN_SUB,
        &json!({ "roles": ["reader"], "scope": "reader", "act": { "sub": "agent-robot-7" } }),
    )
}

/// A service-account token: the `service_account` scope, and the same roles.
fn service_account_token() -> String {
    mint(HUMAN_SUB, &json!({ "roles": ["reader"], "scope": "service_account reader" }))
}

struct Rig {
    server: TestServer,
    client: reqwest::Client,
}

impl Rig {
    async fn gql(&self, query: &str, token: Option<&str>) -> Value {
        let mut req = self
            .client
            .post(format!("{}/graphql", self.server.url))
            .json(&json!({ "query": query }));
        if let Some(t) = token {
            req = req.bearer_auth(t);
        }
        let resp = req.send().await.expect("request");
        let text = resp.text().await.expect("body");
        serde_json::from_str(&text).unwrap_or_else(|e| panic!("non-JSON body {text:?}: {e}"))
    }
}

async fn boot() -> Option<Rig> {
    let url = try_database_url()?;
    let adapter = PostgresAdapter::new(&url).await.expect("adapter");
    provision(&adapter).await;
    drop(adapter);

    let server = Box::pin(temp_env::async_with_vars(
        [(SECRET_ENV, Some(SECRET))],
        TestServer::start_with_rest_writes(
            config(),
            schema(),
            Arc::new(PostgresAdapter::new(&url).await.expect("server adapter")),
        ),
    ))
    .await;
    Some(Rig {
        server,
        client: reqwest::Client::new(),
    })
}

/// `true` when the response is the actor gate's refusal.
///
/// The gate raises `FraiseQLError::Authorization`, which the error sanitizer
/// renders as `code: FORBIDDEN` with a generic message — deliberately, so a
/// refusal does not describe the policy that produced it. That code is what
/// distinguishes it from the *role* gate, which raises a validation error saying
/// the operation does not exist (`the_role_gate_still_runs_first` pins the
/// difference). Matching on prose would break the moment the sanitizer is
/// configured differently; matching on the code is the contract.
fn refused_for_actor(body: &Value) -> bool {
    body["errors"]
        .as_array()
        .is_some_and(|errs| errs.iter().any(|e| e["code"].as_str() == Some("FORBIDDEN")))
}

// ── The control ──────────────────────────────────────────────────────────────

/// Every actor class reaches an operation with no allow-list.
///
/// First, because a server that refused everything would pass every other test
/// here; and second, because it pins that the classification itself works — all
/// three tokens authenticate.
#[tokio::test]
async fn an_unrestricted_operation_is_reachable_by_every_class() {
    if database_url_or_skip("an_unrestricted_operation_is_reachable_by_every_class").is_none() {
        return;
    }
    let rig = Box::pin(boot()).await.unwrap();
    for (name, token) in [
        ("human", human_token()),
        ("agent", agent_token()),
        ("service_account", service_account_token()),
    ] {
        let body = rig.gql("{ openSecrets { id label } }", Some(&token)).await;
        assert!(
            body["data"]["openSecrets"].as_array().is_some_and(|r| r.len() == 2),
            "{name} must reach the unrestricted query: {body}"
        );
    }
}

// ── The gate, on every transport ─────────────────────────────────────────────

/// GraphQL query: the human passes, the other two classes do not.
#[tokio::test]
async fn graphql_refuses_a_restricted_query_to_an_agent() {
    if database_url_or_skip("graphql_refuses_a_restricted_query_to_an_agent").is_none() {
        return;
    }
    let rig = Box::pin(boot()).await.unwrap();

    let allowed = rig.gql("{ humanSecrets { id label } }", Some(&human_token())).await;
    assert!(
        allowed["data"]["humanSecrets"].as_array().is_some_and(|r| r.len() == 2),
        "a human user reaches it: {allowed}"
    );

    for (name, token) in [
        ("agent", agent_token()),
        ("service_account", service_account_token()),
    ] {
        let denied = rig.gql("{ humanSecrets { id label } }", Some(&token)).await;
        assert!(refused_for_actor(&denied), "{name} must be refused by class: {denied}");
        assert!(
            denied["data"]["humanSecrets"].as_array().is_none(),
            "and no row may reach the response: {denied}"
        );
    }
}

/// The delegation case, and the reason `requires_role` cannot express this.
///
/// The agent token carries `roles: ["reader"]` — the human's roles — so the role
/// gate admits it. An implementation that resolved the predicate through
/// `acting_for` would let it through, which is the opposite of what the issue
/// asks for. This is the single test that distinguishes the two designs.
#[tokio::test]
async fn a_delegated_agent_holding_the_role_is_still_refused() {
    if database_url_or_skip("a_delegated_agent_holding_the_role_is_still_refused").is_none() {
        return;
    }
    let rig = Box::pin(boot()).await.unwrap();

    // Same query, same role, different class — and the *role* is what changes
    // nothing here.
    let body = rig.gql("{ humanSecrets { id } }", Some(&agent_token())).await;
    assert!(
        refused_for_actor(&body),
        "an agent acting for a role-holding human is still an agent: {body}"
    );
}

/// A caller lacking the role gets the role gate's enumeration-hiding "not
/// found", not the actor message — the two gates run in that order on purpose.
#[tokio::test]
async fn the_role_gate_still_runs_first() {
    if database_url_or_skip("the_role_gate_still_runs_first").is_none() {
        return;
    }
    let rig = Box::pin(boot()).await.unwrap();
    let roleless = mint(HUMAN_SUB, &json!({ "roles": [] }));

    let body = rig.gql("{ humanSecrets { id } }", Some(&roleless)).await;
    assert!(
        body["errors"]
            .as_array()
            .is_some_and(|e| e[0]["message"].as_str().unwrap_or_default().contains("not found")),
        "a role-less human learns only that the query does not exist: {body}"
    );
    assert!(
        !refused_for_actor(&body),
        "the actor gate must not pre-empt the role gate's enumeration hiding: {body}"
    );
}

/// GraphQL mutation, through the universal mutation chokepoint.
#[tokio::test]
async fn graphql_refuses_a_restricted_mutation_to_an_agent() {
    if database_url_or_skip("graphql_refuses_a_restricted_mutation_to_an_agent").is_none() {
        return;
    }
    let rig = Box::pin(boot()).await.unwrap();
    let m = r#"mutation { createSecret(label: "by-agent") { id label } }"#;

    let denied = rig.gql(m, Some(&agent_token())).await;
    assert!(refused_for_actor(&denied), "{denied}");

    // The refusal is a refusal, not a slow success: nothing was written.
    let rows = rig.gql("{ openSecrets { label } }", Some(&agent_token())).await;
    let labels: Vec<&str> = rows["data"]["openSecrets"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["label"].as_str().unwrap_or_default())
        .collect();
    assert!(
        !labels.contains(&"by-agent"),
        "the refused write must not have happened: {labels:?}"
    );

    // …and the human's identical mutation does write, so the refusal is about
    // the class and not about the mutation being broken.
    let allowed = rig
        .gql(
            r#"mutation { createSecret(label: "by-human") { id label } }"#,
            Some(&human_token()),
        )
        .await;
    assert_eq!(allowed["data"]["createSecret"]["label"], json!("by-human"), "{allowed}");
}

/// The REST transport reaches the database through the same executor gate, so
/// it inherits the predicate with no REST-side code at all.
#[tokio::test]
async fn rest_refuses_the_same_restricted_query() {
    if database_url_or_skip("rest_refuses_the_same_restricted_query").is_none() {
        return;
    }
    let rig = Box::pin(boot()).await.unwrap();

    // The human FIRST: a route that does not exist answers 404 to everyone, and
    // 404 is a client error — so asserting the agent's refusal before knowing the
    // route works is an assertion a typo satisfies.
    let allowed = rig
        .client
        .get(format!("{}/rest/v1/humanSecrets", rig.server.url))
        .bearer_auth(human_token())
        .send()
        .await
        .expect("request");
    let status = allowed.status();
    let body = allowed.text().await.expect("body");
    assert_eq!(status, 200, "the route must exist and serve the human: {body}");
    assert!(body.contains("classified"), "and serve the rows: {body}");

    let denied = rig
        .client
        .get(format!("{}/rest/v1/humanSecrets", rig.server.url))
        .bearer_auth(agent_token())
        .send()
        .await
        .expect("request");
    let status = denied.status();
    let body = denied.text().await.expect("body");
    assert_eq!(status, 403, "REST must refuse the agent at the same gate: {body}");
    assert!(
        !body.contains("classified"),
        "and no restricted row may reach a REST response: {body}"
    );
}

/// The MCP transport, driven through the same `call_tool_authenticated` seam the
/// server mounts.
#[cfg(feature = "mcp")]
#[tokio::test]
async fn mcp_refuses_the_same_restricted_query() {
    use fraiseql_core::{
        runtime::Executor,
        schema::McpConfig,
        security::{AuthConfig, AuthMiddleware},
    };
    use fraiseql_server::{
        mcp::handler::{FraiseQLMcpService, McpTokenValidator},
        routes::graphql::AppState,
    };

    if database_url_or_skip("mcp_refuses_the_same_restricted_query").is_none() {
        return;
    }
    let url = try_database_url().unwrap();
    let adapter = PostgresAdapter::new(&url).await.expect("adapter");
    provision(&adapter).await;

    let state = AppState::new(Arc::new(Executor::new(schema(), Arc::new(adapter))));
    // The same issuer/audience the HTTP mount validates: one token shape drives
    // every transport in this suite, so a validator that disagreed would prove
    // nothing about the gate.
    let validator = McpTokenValidator::Hs256(Arc::new(AuthMiddleware::from_config(AuthConfig {
        issuer: Some(ISSUER.to_string()),
        audience: Some(AUDIENCE.to_string()),
        ..AuthConfig::with_hs256(SECRET)
    })));
    let service = FraiseQLMcpService::new(
        state,
        McpConfig {
            enabled: true,
            require_auth: true,
            ..McpConfig::default()
        },
    )
    .with_token_validator(Some(validator));

    let denied = service
        .call_tool_authenticated(
            "humanSecrets",
            None,
            Some(agent_token()),
            "mcp-p20-agent".to_string(),
            &axum::http::HeaderMap::new(),
        )
        .await;
    assert_eq!(
        denied.is_error,
        Some(true),
        "MCP must refuse the agent at the same gate: {:?}",
        denied.content
    );
    assert!(
        !format!("{:?}", denied.content).contains("classified"),
        "and no row may reach an MCP response: {:?}",
        denied.content
    );

    let allowed = service
        .call_tool_authenticated(
            "humanSecrets",
            None,
            Some(human_token()),
            "mcp-p20-human".to_string(),
            &axum::http::HeaderMap::new(),
        )
        .await;
    assert_ne!(
        allowed.is_error,
        Some(true),
        "the human reaches it over MCP: {:?}",
        allowed.content
    );
}

/// An unauthenticated request never reaches a human-only operation.
///
/// On this mount the `[auth_hs256]` layer refuses it before the executor, so
/// what this asserts is the *outcome* — no row — rather than which gate spoke.
/// That the predicate itself also refuses an unclassified request, instead of
/// falling back to `ActorType::default()` (which is `HumanUser`, and would admit
/// every anonymous caller to a human-only operation), is pinned by
/// `anonymous_is_refused_by_any_non_empty_list` in
/// `security/actor_type/tests.rs` — the two together cover both the mounted path
/// and the one an unauthenticated transport would take.
#[tokio::test]
async fn an_anonymous_request_belongs_to_no_class() {
    if database_url_or_skip("an_anonymous_request_belongs_to_no_class").is_none() {
        return;
    }
    let rig = Box::pin(boot()).await.unwrap();

    let resp = rig
        .client
        .post(format!("{}/graphql", rig.server.url))
        .json(&json!({ "query": "{ humanSecrets { id label } }" }))
        .send()
        .await
        .expect("request");
    let status = resp.status();
    let body = resp.text().await.expect("body");
    assert!(status.is_client_error(), "anonymous must be refused — {status}: {body}");
    assert!(!body.contains("classified"), "and no row may reach it: {body}");
}
