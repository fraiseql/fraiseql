//! Federation integration tests — FraiseQL as a real Apollo Federation subgraph.
//!
//! These tests exercise the full HTTP stack. The in-process tests (SDL, `_entities` by
//! id) start a live FraiseQL server via `TestServer` against the harness Postgres
//! (`DATABASE_URL`, gated by `FEDERATION_TESTS`). The service-backed tests drive an
//! Apollo Router (`ROUTER_URL`) and two FraiseQL subgraphs (`SUBGRAPH_A_URL` /
//! `SUBGRAPH_B_URL`) that the Dagger `federation` suite provisions as bound services;
//! each skips cleanly when its env var is unset.
//!
//! ```sh
//! dagger call test-integration --suite=federation
//! ```
//!
//! **Execution engine:** real FraiseQL executor
//! **Infrastructure:** PostgreSQL + Apollo Router
//! **Parallelism:** safe

#![allow(clippy::print_stdout, clippy::print_stderr)] // Reason: CLI / test / example / bench code prints to stdout/stderr by design
mod common;

use std::sync::Arc;

use fraiseql_core::{db::postgres::PostgresAdapter, schema::CompiledSchema};
use fraiseql_test_utils::failing_adapter::FailingAdapter;
use reqwest::Client;
use serde_json::{Value, json};

use crate::common::server_harness::TestServer;

/// Read an env var, returning `None` (so the caller skips) when unset or blank.
fn env_opt(name: &str) -> Option<String> {
    std::env::var(name).ok().filter(|v| !v.trim().is_empty())
}

/// POST a GraphQL body to `{base}/graphql` and parse the JSON response.
async fn post_gql(http: &Client, base: &str, body: Value) -> Value {
    http.post(format!("{base}/graphql"))
        .json(&body)
        .send()
        .await
        .expect("graphql request failed")
        .json::<Value>()
        .await
        .expect("json parse failed")
}

// ─── Schema helpers ─────────────────────────────────────────────────────────

/// Compiled schema representing a federation-enabled `User` subgraph (id, name).
///
/// Shared with the Dagger `federation` suite's subgraph-A service via the same
/// fixture file, so the in-process test and the bound subgraph run identical schemas.
fn user_schema_with_federation() -> CompiledSchema {
    let json = include_str!("fixtures/federation/schema_users.json");
    CompiledSchema::from_json(json, false).expect("test schema must be valid")
}

// ─── PostgreSQL helpers ──────────────────────────────────────────────────────

async fn setup_users_table(db_url: &str) -> tokio_postgres::Client {
    let (client, conn) = tokio_postgres::connect(db_url, tokio_postgres::NoTls)
        .await
        .expect("connect to harness postgres");
    tokio::spawn(async move { conn.await.ok() });

    client
        .batch_execute(
            r#"CREATE TABLE IF NOT EXISTS "user" (
               id TEXT PRIMARY KEY,
               name TEXT NOT NULL,
               __typename TEXT DEFAULT 'User'
            );"#,
        )
        .await
        .expect("create user table");

    client
}

// ─── Test 1: _service { sdl } ────────────────────────────────────────────────

/// `_service { sdl }` returns SDL with proper inline `@key` directives.
///
/// Does NOT require Docker — uses `FailingAdapter` (no DB needed for SDL generation).
#[tokio::test]
async fn service_sdl_contains_federation_directives() {
    let schema = user_schema_with_federation();
    let adapter = Arc::new(FailingAdapter::new());
    let server = TestServer::start(schema, adapter).await;

    let client = Client::new();
    let resp = client
        .post(format!("{}/graphql", server.url))
        .json(&json!({"query": "{ _service { sdl } }"}))
        .send()
        .await
        .expect("request failed")
        .json::<Value>()
        .await
        .expect("json parse failed");

    assert!(resp["errors"].is_null(), "unexpected errors: {}", resp["errors"]);

    let sdl = resp["data"]["_service"]["sdl"].as_str().expect("_service.sdl must be a string");

    assert!(
        sdl.contains("@key(fields: \"id\")"),
        "SDL must contain inline @key directive, got:\n{sdl}"
    );
    assert!(sdl.contains("type User"), "SDL must define User type");
    assert!(sdl.contains("_entities"), "SDL must declare _entities query");
    assert!(sdl.contains("_service"), "SDL must declare _service query");
    assert!(!sdl.contains("# @key"), "SDL must not contain commented @key: {sdl}");
}

// ─── Test 2: _entities resolves User from PostgreSQL ─────────────────────────

/// `_entities` resolves a `User` entity by ID from a real PostgreSQL database.
#[tokio::test]
async fn entities_resolves_user_by_id() {
    if std::env::var("FEDERATION_TESTS").is_err() {
        eprintln!("Skipping: FEDERATION_TESTS not set");
        return;
    }
    let Some(pg) = fraiseql_test_support::postgres().await else {
        eprintln!("Skipping: no DATABASE_URL (or local-testcontainers)");
        return;
    };
    let db_url = pg.url();

    let pg_client = setup_users_table(db_url).await;
    pg_client
        .execute(
            r#"INSERT INTO "user" (id, name) VALUES ($1, $2) ON CONFLICT DO NOTHING"#,
            &[&"user-alice", &"Alice"],
        )
        .await
        .expect("insert test user");

    let adapter = Arc::new(PostgresAdapter::new(db_url).await.expect("postgres adapter"));

    let schema = user_schema_with_federation();
    let server = TestServer::start(schema, adapter).await;

    let http = Client::new();
    let resp = http
        .post(format!("{}/graphql", server.url))
        .json(&json!({
            "query": "query($representations: [_Any!]!) { _entities(representations: $representations) { ... on User { id name } } }",
            "variables": {
                "representations": [{"__typename": "User", "id": "user-alice"}]
            }
        }))
        .send()
        .await
        .expect("request failed")
        .json::<Value>()
        .await
        .expect("json parse failed");

    assert!(resp["errors"].is_null(), "unexpected errors: {}", resp["errors"]);
    assert_eq!(resp["data"]["_entities"][0]["name"], "Alice", "full response: {resp}");
}

// ─── Test 3: Apollo Router routes through FraiseQL ───────────────────────────

/// Apollo Router, configured with a supergraph pointing at a FraiseQL subgraph,
/// routes a `user(id)` query through to PostgreSQL.
///
/// The Router and the FraiseQL subgraph are provisioned by Dagger as bound services
/// (see the `federation` integration suite); this test drives the Router over HTTP via
/// `ROUTER_URL`. It skips cleanly when that is unset.
#[tokio::test]
async fn apollo_router_routes_query_to_fraiseql_subgraph() {
    let Some(router_url) = env_opt("ROUTER_URL") else {
        eprintln!("Skipping: ROUTER_URL not set (Apollo Router endpoint)");
        return;
    };

    let http = Client::new();
    let resp = http
        .post(format!("{router_url}/graphql"))
        .json(&json!({"query": "{ user(id: \"user-bob\") { id name } }"}))
        .send()
        .await
        .expect("gateway request failed")
        .json::<Value>()
        .await
        .expect("json parse failed");

    assert!(resp["errors"].is_null(), "gateway errors: {}", resp["errors"]);
    assert_eq!(resp["data"]["user"]["name"], "Bob");
}

// ─── Test 4: Cross-subgraph entity resolution (E2E) ────────────────────────

/// Cross-subgraph entity resolution: two FraiseQL subgraphs (provisioned by Dagger as
/// bound services) resolve User data from different perspectives and an internal fan-out
/// validates the plan.
///
/// - Subgraph A owns `User { id, name }` with `@key(fields: "id")`
/// - Subgraph B extends `User { id (external), reviewcount }` with `extend type User @key(fields:
///   "id")`
///
/// This test validates:
/// - Both subgraphs respond to `_service { sdl }`
/// - Subgraph A resolves `_entities` for User by ID (returns name)
/// - Subgraph B resolves `_entities` for User by ID (returns reviewcount)
/// - The two responses can be merged on the `id` key (mini gateway fan-out)
///
/// Drives `SUBGRAPH_A_URL` / `SUBGRAPH_B_URL` over HTTP; skips cleanly when unset.
#[tokio::test]
async fn cross_subgraph_entity_resolution_e2e() {
    let (Some(subgraph_a), Some(subgraph_b)) =
        (env_opt("SUBGRAPH_A_URL"), env_opt("SUBGRAPH_B_URL"))
    else {
        eprintln!("Skipping: SUBGRAPH_A_URL / SUBGRAPH_B_URL not set");
        return;
    };

    let http = Client::new();

    // ── Validate both subgraphs serve federation SDL ────────────────────────

    let sdl_a = post_gql(&http, &subgraph_a, json!({"query": "{ _service { sdl } }"})).await;
    assert!(
        sdl_a["data"]["_service"]["sdl"].is_string(),
        "Subgraph A should serve SDL: {sdl_a}"
    );
    let sdl_b = post_gql(&http, &subgraph_b, json!({"query": "{ _service { sdl } }"})).await;
    assert!(
        sdl_b["data"]["_service"]["sdl"].is_string(),
        "Subgraph B should serve SDL: {sdl_b}"
    );

    // ── Cross-subgraph fan-out: resolve User from both subgraphs ────────────

    // Step 1: Fetch User name from Subgraph A via _entities
    let entities_a = post_gql(
        &http,
        &subgraph_a,
        json!({
            "query": "query($representations: [_Any!]!) { _entities(representations: $representations) { ... on User { id name } } }",
            "variables": {"representations": [{"__typename": "User", "id": "user-1"}]}
        }),
    )
    .await;
    assert!(
        entities_a["errors"].is_null(),
        "Subgraph A entity errors: {}",
        entities_a["errors"]
    );
    assert_eq!(entities_a["data"]["_entities"][0]["name"], "Alice");
    assert_eq!(entities_a["data"]["_entities"][0]["id"], "user-1");

    // Step 2: Fetch User reviewcount from Subgraph B via _entities
    let entities_b = post_gql(
        &http,
        &subgraph_b,
        json!({
            "query": "query($representations: [_Any!]!) { _entities(representations: $representations) { ... on User { id reviewcount } } }",
            "variables": {"representations": [{"__typename": "User", "id": "user-1"}]}
        }),
    )
    .await;
    assert!(
        entities_b["errors"].is_null(),
        "Subgraph B entity errors: {}",
        entities_b["errors"]
    );
    assert_eq!(entities_b["data"]["_entities"][0]["reviewcount"], 42);
    assert_eq!(entities_b["data"]["_entities"][0]["id"], "user-1");

    // Step 3: Merge on `id` key — simulates gateway merge
    let user_a = &entities_a["data"]["_entities"][0];
    let user_b = &entities_b["data"]["_entities"][0];
    assert_eq!(user_a["id"], user_b["id"], "merge key must match");

    let merged = json!({
        "id": user_a["id"],
        "name": user_a["name"],
        "reviewcount": user_b["reviewcount"],
    });
    assert_eq!(merged["name"], "Alice");
    assert_eq!(merged["reviewcount"], 42);
    eprintln!("Cross-subgraph merge successful: {merged}");
}

// ─── Test 5: _entities returns null for missing entity ───────────────────────

/// `_entities` returns `null` (not an error) when an entity is not found.
///
/// `FailingAdapter.execute_raw_query` returns `Ok(vec![])` (empty result set),
/// which the resolver maps to `null` for the missing entity.
/// No Docker or real database required.
#[tokio::test]
async fn entities_returns_null_for_missing_entity() {
    let schema = user_schema_with_federation();
    let adapter = Arc::new(FailingAdapter::new());
    let server = TestServer::start(schema, adapter).await;

    let http = Client::new();
    let resp = http
        .post(format!("{}/graphql", server.url))
        .json(&json!({
            "query": "query($representations: [_Any!]!) { _entities(representations: $representations) { ... on User { id } } }",
            "variables": {
                "representations": [{"__typename": "User", "id": "00000000-0000-0000-0000-000000000000"}]
            }
        }))
        .send()
        .await
        .expect("request failed")
        .json::<Value>()
        .await
        .expect("json parse failed");

    assert!(resp["errors"].is_null(), "unexpected errors: {}", resp["errors"]);
    assert!(
        resp["data"]["_entities"][0].is_null(),
        "missing entity should be null, got: {}",
        resp["data"]["_entities"][0]
    );
}
