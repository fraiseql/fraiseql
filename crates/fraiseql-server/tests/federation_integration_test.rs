//! Federation integration tests — FraiseQL as a real Apollo Federation subgraph.
//!
//! These tests exercise the full HTTP stack: a live FraiseQL server started on an
//! ephemeral port via `TestServer`, queried with `reqwest`.
//!
//! # Running
//!
//! All tests are `#[ignore]` by default because they require Docker
//! (testcontainers pulls PostgreSQL and/or Apollo Router images).
//!
//! ```sh
//! # Run all federation integration tests (Docker must be available):
//! cargo nextest run -p fraiseql-server --test federation_integration_test \
//!   -- --include-ignored
//! ```
//!
//! **Execution engine:** real FraiseQL executor
//! **Infrastructure:** PostgreSQL
//! **Parallelism:** safe

mod common;

use std::sync::Arc;

use fraiseql_core::{db::postgres::PostgresAdapter, schema::CompiledSchema};
use fraiseql_test_utils::failing_adapter::FailingAdapter;
use reqwest::Client;
use serde_json::{Value, json};
use testcontainers_modules::{postgres::Postgres, testcontainers::runners::AsyncRunner};

use crate::common::server_harness::TestServer;

// ─── Schema helpers ─────────────────────────────────────────────────────────

/// Compiled schema representing a federation-enabled `User` subgraph.
///
/// SDL stored in `schema_sdl` so `raw_schema()` returns proper SDL.
fn user_schema_with_federation() -> CompiledSchema {
    // Build via JSON deserialization — avoids coupling to every struct field.
    let json = r#"{
        "types": [
            {
                "name": "User",
                "table": "\"user\"",
                "fields": [
                    {"name": "id",   "field_type": "ID",     "nullable": false},
                    {"name": "name", "field_type": "String", "nullable": false}
                ]
            }
        ],
        "queries": [
            {
                "name": "user",
                "return_type": "User",
                "returns_list": false,
                "nullable": true,
                "arguments": [
                    {"name": "id", "arg_type": "ID", "nullable": false}
                ]
            }
        ],
        "mutations": [],
        "subscriptions": [],
        "federation": {
            "enabled": true,
            "version": "v2",
            "service_name": "users",
            "entities": [
                {"name": "User", "key_fields": ["id"]}
            ]
        },
        "schema_sdl": "type User {\n  id: ID!\n  name: String!\n}\n\ntype Query {\n  user(id: ID!): User\n}\n"
    }"#;

    CompiledSchema::from_json(json).expect("test schema must be valid")
}

// ─── PostgreSQL helpers ──────────────────────────────────────────────────────

async fn setup_users_table(port: u16) -> tokio_postgres::Client {
    let (client, conn) = tokio_postgres::connect(
        &format!("host=127.0.0.1 port={port} user=testuser password=testpw dbname=testdb"),
        tokio_postgres::NoTls,
    )
    .await
    .expect("connect to test postgres");
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
#[ignore = "requires Docker (testcontainers)"]
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

    let sdl = resp["data"]["_service"]["sdl"]
        .as_str()
        .expect("_service.sdl must be a string");

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
#[ignore = "requires Docker (testcontainers pulls postgres image)"]
async fn entities_resolves_user_by_id() {
    let pg = Postgres::default()
        .with_user("testuser")
        .with_password("testpw")
        .with_db_name("testdb")
        .start()
        .await
        .expect("start postgres container");
    let pg_port = pg.get_host_port_ipv4(5432).await.expect("postgres port");

    let pg_client = setup_users_table(pg_port).await;
    pg_client
        .execute(
            r#"INSERT INTO "user" (id, name) VALUES ($1, $2) ON CONFLICT DO NOTHING"#,
            &[&"user-alice", &"Alice"],
        )
        .await
        .expect("insert test user");

    let db_url = format!("postgresql://testuser:testpw@127.0.0.1:{pg_port}/testdb");
    let adapter =
        Arc::new(PostgresAdapter::new(&db_url).await.expect("postgres adapter"));

    let schema = user_schema_with_federation();
    let server = TestServer::start(schema, adapter).await;

    let http = Client::new();
    let resp = http
        .post(format!("{}/graphql", server.url))
        .json(&json!({
            "query": "query($repr: [_Any!]!) { _entities(representations: $repr) { ... on User { id name } } }",
            "variables": {
                "repr": [{"__typename": "User", "id": "user-alice"}]
            }
        }))
        .send()
        .await
        .expect("request failed")
        .json::<Value>()
        .await
        .expect("json parse failed");

    assert!(resp["errors"].is_null(), "unexpected errors: {}", resp["errors"]);
    assert_eq!(
        resp["data"]["_entities"][0]["name"], "Alice",
        "full response: {resp}"
    );
}

// ─── Test 3: Apollo Router routes through FraiseQL ───────────────────────────

/// Apollo Router, configured with a supergraph pointing at FraiseQL, routes a
/// query through to PostgreSQL.
///
/// Requires Docker with `ghcr.io/apollographql/router:v1.45.0` available.
/// Uses Linux host networking (`--network host`) so the Router container can
/// reach the FraiseQL server on 127.0.0.1.
#[tokio::test]
#[ignore = "requires Docker (testcontainers pulls Apollo Router image + postgres)"]
async fn apollo_router_routes_query_to_fraiseql_subgraph() {
    use testcontainers::{GenericImage, ImageExt as _, core::WaitFor};

    // Start PostgreSQL
    let pg = Postgres::default()
        .with_user("testuser")
        .with_password("testpw")
        .with_db_name("testdb")
        .start()
        .await
        .expect("start postgres container");
    let pg_port = pg.get_host_port_ipv4(5432).await.expect("postgres port");

    let pg_client = setup_users_table(pg_port).await;
    pg_client
        .execute(
            r#"INSERT INTO "user" (id, name) VALUES ($1, $2) ON CONFLICT DO NOTHING"#,
            &[&"user-bob", &"Bob"],
        )
        .await
        .expect("insert test user");

    let db_url = format!("postgresql://testuser:testpw@127.0.0.1:{pg_port}/testdb");
    let adapter =
        Arc::new(PostgresAdapter::new(&db_url).await.expect("postgres adapter"));

    let schema = user_schema_with_federation();
    let server = TestServer::start(schema, adapter).await;

    // Build supergraph SDL: replace the __SUBGRAPH_URL__ placeholder with the
    // real server URL so Apollo Router knows where to send subgraph requests.
    let supergraph = include_str!(
        "../../fraiseql-core/tests/federation/fixtures/supergraph_single.graphql"
    )
    .replace("__SUBGRAPH_URL__", &format!("{}/graphql", server.url));

    let supergraph_file = tempfile::NamedTempFile::new().expect("tmpfile");
    std::fs::write(supergraph_file.path(), &supergraph).expect("write supergraph");

    // Apollo Router on host network so it can reach the FraiseQL server on 127.0.0.1.
    // The wait condition checks stderr for "GraphQL endpoint exposed".
    let _router = GenericImage::new("ghcr.io/apollographql/router", "v1.45.0")
        .with_wait_for(WaitFor::message_on_stderr("GraphQL endpoint exposed"))
        .with_network("host")
        .with_env_var("APOLLO_ROUTER_SUPERGRAPH_PATH", "/supergraph.graphql")
        .start()
        .await
        .expect("start Apollo Router container");

    let _ = supergraph_file; // keep alive until router is done

    let http = Client::new();
    let resp = http
        .post("http://127.0.0.1:4000/graphql")
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

// ─── Test 4: _entities returns null for missing entity ───────────────────────

/// `_entities` returns `null` (not an error) when an entity is not found.
///
/// `FailingAdapter.execute_raw_query` returns `Ok(vec![])` (empty result set),
/// which the resolver maps to `null` for the missing entity.
/// No Docker or real database required.
#[tokio::test]
#[ignore = "requires Docker (testcontainers)"]
async fn entities_returns_null_for_missing_entity() {
    let schema = user_schema_with_federation();
    let adapter = Arc::new(FailingAdapter::new());
    let server = TestServer::start(schema, adapter).await;

    let http = Client::new();
    let resp = http
        .post(format!("{}/graphql", server.url))
        .json(&json!({
            "query": "query($repr: [_Any!]!) { _entities(representations: $repr) { ... on User { id } } }",
            "variables": {
                "repr": [{"__typename": "User", "id": "00000000-0000-0000-0000-000000000000"}]
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
