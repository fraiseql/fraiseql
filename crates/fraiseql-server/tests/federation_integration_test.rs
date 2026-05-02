//! Federation integration tests — FraiseQL as a real Apollo Federation subgraph.
//!
//! These tests exercise the full HTTP stack: a live FraiseQL server started on an
//! ephemeral port via `TestServer`, queried with `reqwest`.
//!
//! # Running
//!
//! Tests requiring Docker are guarded by the `FEDERATION_TESTS` environment
//! variable and skip automatically when it is not set.
//!
//! ```sh
//! # Run all federation integration tests (Docker must be available):
//! FEDERATION_TESTS=1 cargo nextest run -p fraiseql-server --test federation_integration_test
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
                "sql_source": "v_user",
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
    let adapter = Arc::new(PostgresAdapter::new(&db_url).await.expect("postgres adapter"));

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
    assert_eq!(resp["data"]["_entities"][0]["name"], "Alice", "full response: {resp}");
}

// ─── Test 3: Apollo Router routes through FraiseQL ───────────────────────────

/// Apollo Router, configured with a supergraph pointing at FraiseQL, routes a
/// query through to PostgreSQL.
///
/// Requires Docker with `ghcr.io/apollographql/router:v1.45.0` available.
/// Uses Linux host networking (`--network host`) so the Router container can
/// reach the FraiseQL server on 127.0.0.1.
#[tokio::test]
async fn apollo_router_routes_query_to_fraiseql_subgraph() {
    use testcontainers::{GenericImage, ImageExt as _, core::WaitFor};

    if std::env::var("FEDERATION_TESTS").is_err() {
        eprintln!("Skipping: FEDERATION_TESTS not set");
        return;
    }

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
    let adapter = Arc::new(PostgresAdapter::new(&db_url).await.expect("postgres adapter"));

    let schema = user_schema_with_federation();
    let server = TestServer::start(schema, adapter).await;

    // Build supergraph SDL: replace the __SUBGRAPH_URL__ placeholder with the
    // real server URL so Apollo Router knows where to send subgraph requests.
    let supergraph =
        include_str!("../../fraiseql-core/tests/federation/fixtures/supergraph_single.graphql")
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

// ─── Test 4: Cross-subgraph entity resolution (E2E) ────────────────────────

/// Cross-subgraph entity resolution: two FraiseQL subgraphs resolve User data
/// from different perspectives and an internal fan-out validates the plan.
///
/// - Subgraph A owns `User { id, name }` with `@key(fields: "id")`
/// - Subgraph B extends `User { id (external), reviewCount }` with `extend type User @key(fields: "id")`
///
/// This test validates:
/// - Both subgraphs start and respond to `_service { sdl }`
/// - Subgraph A resolves `_entities` for User by ID (returns name)
/// - Subgraph B resolves `_entities` for User by ID (returns reviewCount)
/// - The two responses can be merged on the `id` key (mini gateway fan-out)
///
/// Run with: `FRAISEQL_FEDERATION_E2E=1 FEDERATION_TESTS=1 cargo nextest run -p fraiseql-server --test federation_integration_test -- cross_subgraph`
#[tokio::test]
#[ignore = "requires Docker + PostgreSQL (FRAISEQL_FEDERATION_E2E=1)"]
async fn cross_subgraph_entity_resolution_e2e() {
    if std::env::var("FRAISEQL_FEDERATION_E2E").is_err() {
        eprintln!("Skipping: FRAISEQL_FEDERATION_E2E not set");
        return;
    }

    // ── Subgraph A: User(id, name) ──────────────────────────────────────────

    let pg_a = Postgres::default()
        .with_user("testuser")
        .with_password("testpw")
        .with_db_name("testdb")
        .start()
        .await
        .expect("start postgres A");
    let pg_a_port = pg_a.get_host_port_ipv4(5432).await.expect("pg A port");

    let pg_a_client = setup_users_table(pg_a_port).await;
    pg_a_client
        .execute(
            r#"INSERT INTO "user" (id, name) VALUES ($1, $2) ON CONFLICT DO NOTHING"#,
            &[&"user-1", &"Alice"],
        )
        .await
        .expect("insert user into subgraph A");

    let db_url_a = format!("postgresql://testuser:testpw@127.0.0.1:{pg_a_port}/testdb");
    let adapter_a = Arc::new(PostgresAdapter::new(&db_url_a).await.expect("pg adapter A"));
    let schema_a = user_schema_with_federation();
    let server_a = TestServer::start(schema_a, adapter_a).await;

    // ── Subgraph B: User(id @external, reviewCount) ─────────────────────────

    let pg_b = Postgres::default()
        .with_user("testuser")
        .with_password("testpw")
        .with_db_name("testdb")
        .start()
        .await
        .expect("start postgres B");
    let pg_b_port = pg_b.get_host_port_ipv4(5432).await.expect("pg B port");

    // Subgraph B stores review counts in its own user table
    let pg_b_client = {
        let (client, conn) = tokio_postgres::connect(
            &format!("host=127.0.0.1 port={pg_b_port} user=testuser password=testpw dbname=testdb"),
            tokio_postgres::NoTls,
        )
        .await
        .expect("connect to pg B");
        tokio::spawn(async move { conn.await.ok() });
        client
    };

    pg_b_client
        .batch_execute(
            r#"CREATE TABLE IF NOT EXISTS "user" (
               id TEXT PRIMARY KEY,
               name TEXT NOT NULL DEFAULT '',
               review_count INTEGER NOT NULL DEFAULT 0,
               __typename TEXT DEFAULT 'User'
            );"#,
        )
        .await
        .expect("create user table on B");
    pg_b_client
        .execute(
            r#"INSERT INTO "user" (id, review_count) VALUES ($1, $2) ON CONFLICT DO NOTHING"#,
            &[&"user-1", &42i32],
        )
        .await
        .expect("insert review data into subgraph B");

    let schema_b_json = r#"{
        "types": [
            {
                "name": "User",
                "table": "\"user\"",
                "sql_source": "\"user\"",
                "fields": [
                    {"name": "id",          "field_type": "ID",  "nullable": false},
                    {"name": "reviewCount", "field_type": "Int", "nullable": false, "column": "review_count"}
                ]
            }
        ],
        "queries": [],
        "mutations": [],
        "subscriptions": [],
        "federation": {
            "enabled": true,
            "version": "v2",
            "service_name": "reviews",
            "entities": [
                {"name": "User", "key_fields": ["id"]}
            ]
        },
        "schema_sdl": "extend type User @key(fields: \"id\") {\n  id: ID! @external\n  reviewCount: Int!\n}\n"
    }"#;
    let schema_b = CompiledSchema::from_json(schema_b_json).expect("schema B valid");
    let db_url_b = format!("postgresql://testuser:testpw@127.0.0.1:{pg_b_port}/testdb");
    let adapter_b = Arc::new(PostgresAdapter::new(&db_url_b).await.expect("pg adapter B"));
    let server_b = TestServer::start(schema_b, adapter_b).await;

    // ── Validate both subgraphs serve federation SDL ────────────────────────

    let http = Client::new();

    let sdl_a = http
        .post(format!("{}/graphql", server_a.url))
        .json(&json!({"query": "{ _service { sdl } }"}))
        .send()
        .await
        .expect("SDL A")
        .json::<Value>()
        .await
        .expect("parse A");
    assert!(
        sdl_a["data"]["_service"]["sdl"].is_string(),
        "Subgraph A should serve SDL: {sdl_a}"
    );

    let sdl_b = http
        .post(format!("{}/graphql", server_b.url))
        .json(&json!({"query": "{ _service { sdl } }"}))
        .send()
        .await
        .expect("SDL B")
        .json::<Value>()
        .await
        .expect("parse B");
    assert!(
        sdl_b["data"]["_service"]["sdl"].is_string(),
        "Subgraph B should serve SDL: {sdl_b}"
    );

    // ── Cross-subgraph fan-out: resolve User from both subgraphs ────────────

    // Step 1: Fetch User name from Subgraph A via _entities
    let entities_a = http
        .post(format!("{}/graphql", server_a.url))
        .json(&json!({
            "query": "query($repr: [_Any!]!) { _entities(representations: $repr) { ... on User { id name } } }",
            "variables": {"repr": [{"__typename": "User", "id": "user-1"}]}
        }))
        .send()
        .await
        .expect("entities A")
        .json::<Value>()
        .await
        .expect("parse entities A");

    assert!(
        entities_a["errors"].is_null(),
        "Subgraph A entity errors: {}",
        entities_a["errors"]
    );
    assert_eq!(entities_a["data"]["_entities"][0]["name"], "Alice");
    assert_eq!(entities_a["data"]["_entities"][0]["id"], "user-1");

    // Step 2: Fetch User reviewCount from Subgraph B via _entities
    let entities_b = http
        .post(format!("{}/graphql", server_b.url))
        .json(&json!({
            "query": "query($repr: [_Any!]!) { _entities(representations: $repr) { ... on User { id reviewCount } } }",
            "variables": {"repr": [{"__typename": "User", "id": "user-1"}]}
        }))
        .send()
        .await
        .expect("entities B")
        .json::<Value>()
        .await
        .expect("parse entities B");

    assert!(
        entities_b["errors"].is_null(),
        "Subgraph B entity errors: {}",
        entities_b["errors"]
    );
    assert_eq!(entities_b["data"]["_entities"][0]["reviewCount"], 42);
    assert_eq!(entities_b["data"]["_entities"][0]["id"], "user-1");

    // Step 3: Merge on `id` key — simulates gateway merge
    let user_a = &entities_a["data"]["_entities"][0];
    let user_b = &entities_b["data"]["_entities"][0];
    assert_eq!(user_a["id"], user_b["id"], "merge key must match");

    let merged = json!({
        "id": user_a["id"],
        "name": user_a["name"],
        "reviewCount": user_b["reviewCount"],
    });
    assert_eq!(merged["name"], "Alice");
    assert_eq!(merged["reviewCount"], 42);
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
