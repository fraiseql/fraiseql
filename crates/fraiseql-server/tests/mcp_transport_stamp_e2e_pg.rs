//! #376: MCP-originated writes are attributable in the audit trail, end to end
//! against PostgreSQL — and HS256 deployments can authenticate MCP calls at all.
//!
//! Drives `FraiseQLMcpService::call_tool_authenticated` (the documented testable
//! seam under `ServerHandler::call_tool`) with a locally-minted HS256 Bearer
//! token — exercising the #376 auth-parity path (`McpTokenValidator::Hs256`;
//! before it, MCP accepted only OIDC and an `[auth_hs256]` deployment could not
//! authenticate MCP calls) — through the real executor into the change-log
//! outbox, and asserts on the recorded row: `extra_metadata.transport = "mcp"`
//! plus the #390 actor columns.
//!
//! Self-skips when no `DATABASE_URL` is set (no `#[ignore]`), so it is inert in
//! the database-free `test` leg and runs in the Dagger `integration: server`
//! suite.
//!
//! **Execution engine:** `PostgreSQL` · **Infrastructure:** `DATABASE_URL` ·
//! **Parallelism:** recreates its own `p29_mcp` schema AND the shared
//! `core.tb_entity_change_log` (this suite is *about* the outbox row) → run
//! `--test-threads=1`, never in-process with another changelog-owning binary.
#![cfg(feature = "mcp")]
#![allow(clippy::unwrap_used, clippy::panic, clippy::print_stderr)] // Reason: test code — panics and skip diagnostics are acceptable

use std::sync::Arc;

use fraiseql_core::{
    db::postgres::PostgresAdapter,
    prelude::DatabaseAdapter as _,
    runtime::Executor,
    schema::{
        ArgumentDefinition, CompiledSchema, FieldDefinition, FieldType, McpConfig,
        MutationDefinition, MutationOperation, TypeDefinition,
    },
    security::{AuthConfig, AuthMiddleware},
};
use fraiseql_server::{
    mcp::handler::{FraiseQLMcpService, McpTokenValidator},
    routes::graphql::AppState,
};
use fraiseql_test_support::try_database_url;
use serde_json::{Value, json};

const SCHEMA: &str = "p29_mcp";
const SECRET: &str = "p29-mcp-secret-0123456789-0123456789";
const HUMAN_SUB: &str = "5a1e0000-0000-4000-8000-000000000376";

async fn exec(adapter: &PostgresAdapter, sql: &str) {
    let _: Vec<std::collections::HashMap<String, Value>> = adapter
        .execute_raw_query(sql)
        .await
        .unwrap_or_else(|e| panic!("fixture `{sql}`: {e}"));
}

/// Fixture: `mutation_response` types, a fresh change-log table, and one
/// `createItem` mutation function in an own schema.
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
        format!("CREATE TABLE {SCHEMA}.tb_item (id uuid PRIMARY KEY, label text NOT NULL)"),
        format!(
            "CREATE VIEW {SCHEMA}.v_item AS SELECT id, jsonb_build_object('id', id, 'label', \
             label) AS data FROM {SCHEMA}.tb_item"
        ),
        format!(
            "CREATE FUNCTION {SCHEMA}.fn_create_item(p_label text) \
             RETURNS app.mutation_response LANGUAGE plpgsql AS $$ \
             DECLARE v app.mutation_response; n uuid; BEGIN \
             n := gen_random_uuid(); \
             INSERT INTO {SCHEMA}.tb_item (id, label) VALUES (n, p_label); \
             v.succeeded := true; v.state_changed := true; v.message := 'created'; \
             v.entity_type := 'McpItem'; v.entity_id := n; \
             v.entity := jsonb_build_object('id', n, 'label', p_label); \
             RETURN v; END; $$"
        ),
    ];
    // #942/#982: the change-log table comes from the ONE shared provisioner
    // (migration-08 contract), one statement per exec call.
    let mut stmts = stmts;
    stmts.extend(fraiseql_test_support::changelog::entity_change_log_provision_statements());

    for stmt in stmts {
        exec(adapter, &stmt).await;
    }
}

fn schema() -> CompiledSchema {
    let mut schema = CompiledSchema::new();

    let mut item = TypeDefinition::new("McpItem", format!("{SCHEMA}.v_item"));
    item.fields = vec![
        FieldDefinition::new("id", FieldType::Id),
        FieldDefinition::new("label", FieldType::String),
    ];
    schema.types.push(item);

    let mut create = MutationDefinition::new("createItem", "McpItem");
    create.sql_source = Some(format!("{SCHEMA}.fn_create_item"));
    create.operation = MutationOperation::Insert {
        table: "tb_item".to_string(),
    };
    create.arguments = vec![ArgumentDefinition::new("label", FieldType::String)];
    schema.mutations.push(create);

    schema.build_indexes();
    schema
}

fn mcp_config() -> McpConfig {
    McpConfig {
        enabled: true,
        require_auth: true,
        ..McpConfig::default()
    }
}

fn hs256_validator() -> McpTokenValidator {
    McpTokenValidator::Hs256(Arc::new(AuthMiddleware::from_config(AuthConfig::with_hs256(SECRET))))
}

fn mint_token(sub: &str) -> String {
    use jsonwebtoken::{Algorithm, EncodingKey, Header, encode};
    let now = i64::try_from(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system time")
            .as_secs(),
    )
    .expect("epoch seconds fit i64");
    let claims = json!({ "sub": sub, "iat": now, "exp": now + 3600 });
    encode(
        &Header::new(Algorithm::HS256),
        &claims,
        &EncodingKey::from_secret(SECRET.as_bytes()),
    )
    .expect("mint token")
}

async fn setup() -> Option<(PostgresAdapter, FraiseQLMcpService<PostgresAdapter>)> {
    let url = try_database_url()?;
    let adapter = PostgresAdapter::new(&url).await.expect("connect");
    provision(&adapter).await;
    let state = AppState::new(Arc::new(Executor::new(schema(), Arc::new(adapter.clone()))));
    let service =
        FraiseQLMcpService::new(state, mcp_config()).with_token_validator(Some(hs256_validator()));
    Some((adapter, service))
}

/// #967: per-thread continuity accumulates across calls, and a client-controlled
/// `mcp-session-id` partitions only its **own** principal's threads.
///
/// The second half is the security property. The header is whatever the client
/// sends, so if the store were keyed on it, one caller could read and overwrite
/// another's durable thread by sending their id. Two principals sending the
/// *identical* header must therefore see entirely separate histories — asserted
/// here against a real store, through the seam the transport actually calls.
///
/// The first half is what makes the second meaningful: without it, a
/// server that recorded nothing would pass the isolation assertion trivially.
#[tokio::test]
async fn session_continuity_accumulates_and_stays_scoped_to_the_principal() {
    use fraiseql_auth::session_state::{
        InMemorySessionStateStore, SessionState, SessionStateBackend,
    };

    /// The thread history a call came back with, as reported in `_meta`.
    fn thread_of(result: &rmcp::model::CallToolResult) -> serde_json::Value {
        result
            .meta
            .as_ref()
            .and_then(|m| m.get("fraiseql/session").cloned())
            .unwrap_or(serde_json::Value::Null)
    }

    let Some((adapter, _)) = setup().await else {
        eprintln!("SKIP session_continuity_accumulates_and_stays_scoped_to_the_principal");
        return;
    };
    let state = AppState::new(Arc::new(Executor::new(schema(), Arc::new(adapter.clone()))));
    let store = Arc::new(SessionState::new(
        SessionStateBackend::InMemory(InMemorySessionStateStore::default()),
        3600,
    ));
    let service = FraiseQLMcpService::new(
        state,
        McpConfig {
            session_state: true,
            ..mcp_config()
        },
    )
    .with_token_validator(Some(hs256_validator()))
    .with_session_state(Some(Arc::clone(&store)));

    let mut headers = axum::http::HeaderMap::new();
    headers.insert("mcp-session-id", "shared-thread-id".parse().unwrap());

    // Alice calls twice on her thread.
    let alice = mint_token(HUMAN_SUB);
    let mut alice_last = serde_json::Value::Null;
    for n in 0..2 {
        let mut args = serde_json::Map::new();
        args.insert("label".to_string(), json!(format!("alice-{n}")));
        let result = service
            .call_tool_authenticated(
                "createItem",
                Some(&args),
                Some(alice.clone()),
                format!("mcp-p29-sess-alice-{n}"),
                &headers,
            )
            .await;
        assert_ne!(result.is_error, Some(true), "alice call {n}: {:?}", result.content);
        alice_last = thread_of(&result);
    }
    let alice_calls = alice_last["calls"].as_array().cloned().unwrap_or_default();
    assert_eq!(
        alice_calls.len(),
        2,
        "the thread accumulates across calls, it does not reset: {alice_last}"
    );

    // Bob calls once, sending the SAME session header.
    let bob = mint_token("5a1e0000-0000-4000-8000-000000000967");
    let mut bob_args = serde_json::Map::new();
    bob_args.insert("label".to_string(), json!("bob-0"));
    let result = service
        .call_tool_authenticated(
            "createItem",
            Some(&bob_args),
            Some(bob),
            "mcp-p29-sess-bob".to_string(),
            &headers,
        )
        .await;
    assert_ne!(result.is_error, Some(true), "bob call: {:?}", result.content);
    let bob_thread = thread_of(&result);
    let bob_calls = bob_thread["calls"].as_array().cloned().unwrap_or_default();

    assert_eq!(
        bob_calls.len(),
        1,
        "bob's first call must see HIS thread, not alice's two entries — the header is \
         client-controlled and must not address another principal's store: {bob_thread}"
    );
    assert_eq!(
        bob_thread["threadId"],
        serde_json::json!("shared-thread-id"),
        "…while still being the thread he asked for: {bob_thread}"
    );

    // …and alice's thread was not disturbed by bob writing to the same header.
    let mut after_args = serde_json::Map::new();
    after_args.insert("label".to_string(), json!("alice-2"));
    let after = service
        .call_tool_authenticated(
            "createItem",
            Some(&after_args),
            Some(alice),
            "mcp-p29-sess-alice-2".to_string(),
            &headers,
        )
        .await;
    let after_calls = thread_of(&after)["calls"].as_array().cloned().unwrap_or_default();
    assert_eq!(after_calls.len(), 3, "alice's thread continued from 2, unaffected by bob");
}

/// #967: with `[mcp] session_state = false` — the default — nothing is stored and
/// no thread is reported.
///
/// The opt-in is the feature: a deployment that upgrades must not silently start
/// writing an agent's call history to a durable store.
#[tokio::test]
async fn continuity_is_off_unless_the_operator_turns_it_on() {
    use fraiseql_auth::session_state::{
        InMemorySessionStateStore, SessionState, SessionStateBackend,
    };

    let Some((adapter, _)) = setup().await else {
        eprintln!("SKIP continuity_is_off_unless_the_operator_turns_it_on");
        return;
    };
    let state = AppState::new(Arc::new(Executor::new(schema(), Arc::new(adapter.clone()))));
    let store = Arc::new(SessionState::new(
        SessionStateBackend::InMemory(InMemorySessionStateStore::default()),
        3600,
    ));
    // The store IS bound; only the flag is off.
    let service = FraiseQLMcpService::new(state, mcp_config())
        .with_token_validator(Some(hs256_validator()))
        .with_session_state(Some(store));

    let mut headers = axum::http::HeaderMap::new();
    headers.insert("mcp-session-id", "some-thread".parse().unwrap());

    let mut args = serde_json::Map::new();
    args.insert("label".to_string(), json!("off"));
    let result = service
        .call_tool_authenticated(
            "createItem",
            Some(&args),
            Some(mint_token(HUMAN_SUB)),
            "mcp-p29-sess-off".to_string(),
            &headers,
        )
        .await;
    assert_ne!(result.is_error, Some(true), "the call itself still works");
    assert!(
        result.meta.as_ref().is_none_or(|m| m.get("fraiseql/session").is_none()),
        "no thread is reported when [mcp] session_state is off: {:?}",
        result.meta
    );
}

/// The headline: an HS256-authenticated MCP mutation succeeds (auth parity) and
/// its change-log row is attributable — `extra_metadata.transport = "mcp"` plus
/// the #390 actor stamp — while nothing else about the envelope is disturbed.
#[tokio::test]
async fn mcp_mutation_records_transport_and_actor() {
    let Some((adapter, service)) = setup().await else {
        eprintln!("SKIP mcp_mutation_records_transport_and_actor: no DATABASE_URL");
        return;
    };

    let mut args = serde_json::Map::new();
    args.insert("label".to_string(), json!("via-mcp"));
    let result = service
        .call_tool_authenticated(
            "createItem",
            Some(&args),
            Some(mint_token(HUMAN_SUB)),
            "mcp-p29-stamp".to_string(),
            &axum::http::HeaderMap::new(),
        )
        .await;
    assert_ne!(
        result.is_error,
        Some(true),
        "HS256-authenticated MCP mutation must succeed: {:?}",
        result.content
    );

    let rows = adapter
        .execute_raw_query(
            "SELECT extra_metadata->>'transport' AS transport, actor_type \
             FROM core.tb_entity_change_log \
             WHERE object_data->>'label' = 'via-mcp'",
        )
        .await
        .unwrap();
    assert_eq!(rows.len(), 1, "exactly one outbox row for the MCP write");
    assert_eq!(
        rows[0].get("transport"),
        Some(&json!("mcp")),
        "MCP-originated write is tagged transport=mcp in the audit trail"
    );
    assert_eq!(
        rows[0].get("actor_type"),
        Some(&json!("human_user")),
        "the #390 actor stamp rides the same context"
    );
}

/// The auth control: with `require_auth = true`, an anonymous call is refused
/// and writes nothing — proving the success above went through validation.
#[tokio::test]
async fn anonymous_call_is_refused_and_writes_nothing() {
    let Some((adapter, service)) = setup().await else {
        eprintln!("SKIP anonymous_call_is_refused_and_writes_nothing: no DATABASE_URL");
        return;
    };

    let mut args = serde_json::Map::new();
    args.insert("label".to_string(), json!("via-anon"));
    let result = service
        .call_tool_authenticated(
            "createItem",
            Some(&args),
            None,
            "mcp-p29-anon".to_string(),
            &axum::http::HeaderMap::new(),
        )
        .await;
    assert_eq!(result.is_error, Some(true), "anonymous MCP call must be refused");

    let rows = adapter
        .execute_raw_query(
            "SELECT count(*) AS n FROM core.tb_entity_change_log \
             WHERE object_data->>'label' = 'via-anon'",
        )
        .await
        .unwrap();
    assert_eq!(rows[0].get("n"), Some(&json!(0)), "a refused call reaches no audit row");
}

/// An invalid HS256 token is refused with the sanitized message — parity with
/// how an invalid OIDC token behaves.
#[tokio::test]
async fn invalid_hs256_token_is_refused() {
    let Some((adapter, service)) = setup().await else {
        eprintln!("SKIP invalid_hs256_token_is_refused: no DATABASE_URL");
        return;
    };
    drop(adapter);

    let result = service
        .call_tool_authenticated(
            "createItem",
            None,
            Some("not-a-jwt".to_string()),
            "mcp-p29-bad".to_string(),
            &axum::http::HeaderMap::new(),
        )
        .await;
    assert_eq!(result.is_error, Some(true), "garbage token must be refused");
    let text = result
        .content
        .first()
        .and_then(|c| c.as_text().map(|t| t.text.clone()))
        .unwrap_or_default();
    assert!(
        text.contains("Invalid or expired"),
        "sanitized refusal, not a validator internals dump: {text}"
    );
}
