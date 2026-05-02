//! Phase 20 Cycle 7 — End-to-End Pipeline Integration Test
//!
//! Proves the full compile → server → HTTP pipeline works without manual
//! intervention:
//!
//! 1. Generate fixture files in a temp directory (fraiseql.toml + TOML-only
//!    type/query definitions)
//! 2. Compile via `fraiseql_cli::commands::compile::compile_to_schema()`
//! 3. Spin up testcontainers PostgreSQL
//! 4. Apply DDL (table + JSONB view matching `sql_source`)
//! 5. Construct `PostgresAdapter` → `Server::new()`
//! 6. Bind to port 0, spawn as background task
//! 7. POST `{ users { id name } }` to `/graphql`
//! 8. Assert the seeded row is present in the response
//!
//! # Running
//!
//! ```bash
//! FRAISEQL_PIPELINE_E2E=1 cargo test -p fraiseql-server \
//!     --test pipeline_e2e_test -- --ignored
//! ```

#![allow(clippy::unwrap_used)] // Reason: test code — panics are acceptable
#![allow(clippy::missing_panics_doc)] // Reason: test helpers
#![allow(clippy::missing_errors_doc)] // Reason: test helpers
#![allow(missing_docs)] // Reason: test file

use std::{sync::Arc, time::Duration};

use fraiseql_cli::commands::compile::{CompileOptions, compile_to_schema};
use fraiseql_core::db::postgres::PostgresAdapter;
use fraiseql_server::{Server, ServerConfig};
use reqwest::Client;
use serde_json::Value;
use tempfile::TempDir;
use testcontainers_modules::{postgres::Postgres, testcontainers::runners::AsyncRunner};
use tokio::{net::TcpListener, sync::oneshot};

// ── Fixture helpers ───────────────────────────────────────────────────────────

/// Minimal `fraiseql.toml` defining a `User` type and `users` list query.
///
/// Uses TOML-only mode (no separate type/query JSON files) so the test has a
/// single file to create.  The `sql_source = "v_users"` view is created by
/// `apply_ddl()` below.
const FRAISEQL_TOML: &str = r#"
[schema]
name = "pipeline-e2e-test"
version = "1.0.0"
database_target = "postgresql"

[types.User]
sql_source = "v_users"

[types.User.fields.id]
type = "Int"
nullable = false

[types.User.fields.name]
type = "String"
nullable = false

[queries.users]
return_type = "User"
return_array = true
sql_source = "v_users"
"#;

/// Write fixture files to `dir` and return the absolute path to `fraiseql.toml`.
fn write_fixtures(dir: &TempDir) -> String {
    let toml_path = dir.path().join("fraiseql.toml");
    std::fs::write(&toml_path, FRAISEQL_TOML).expect("write fraiseql.toml");
    toml_path.to_str().expect("valid UTF-8 path").to_string()
}

// ── Database helpers ──────────────────────────────────────────────────────────

/// Apply DDL to the running PostgreSQL container.
///
/// Creates:
/// - `tb_users` — source table with `id` and `name` columns
/// - `v_users` — JSONB view (matching `sql_source = "v_users"`) that
///   FraiseQL's executor queries via `SELECT data FROM "v_users"`
/// - One seeded row `(name = 'Alice')`
async fn apply_ddl(port: u16) {
    let (client, conn) = tokio_postgres::connect(
        &format!("host=127.0.0.1 port={port} user=testuser password=testpw dbname=testdb"),
        tokio_postgres::NoTls,
    )
    .await
    .expect("connect to testcontainers postgres");

    tokio::spawn(async move { conn.await.ok() });

    client
        .batch_execute(
            r"
            CREATE TABLE tb_users (
                id   SERIAL PRIMARY KEY,
                name TEXT NOT NULL
            );

            INSERT INTO tb_users (name) VALUES ('Alice');

            CREATE VIEW v_users AS
            SELECT jsonb_build_object(
                'id',   id,
                'name', name
            ) AS data
            FROM tb_users;
            ",
        )
        .await
        .expect("apply DDL");
}

// ── TestPipeline — reusable E2E harness ─────────────────────────────────────

/// Reusable in-process server started from a compiled schema and a live
/// PostgreSQL adapter.  Shuts down when dropped.
struct TestPipeline {
    pub url:       String,
    _shutdown:     oneshot::Sender<()>,
    // Keep the temp directory alive for the full test lifetime.
    _fixtures:     TempDir,
}

impl TestPipeline {
    /// Compile fixtures, spin up server on an ephemeral port, return the
    /// running pipeline handle.
    async fn start(fixtures: TempDir, toml_path: &str, db_url: &str) -> Self {
        // ── 1. Compile schema ──────────────────────────────────────────────
        let opts = CompileOptions::new(toml_path);
        let (schema, _report) = compile_to_schema(opts)
            .await
            .expect("compile_to_schema must succeed");

        // ── 2. Build PostgresAdapter ───────────────────────────────────────
        let adapter = Arc::new(
            PostgresAdapter::new(db_url)
                .await
                .expect("PostgresAdapter::new must succeed"),
        );

        // ── 3. Construct server ────────────────────────────────────────────
        let config = ServerConfig::default();
        let server = Server::new(config, schema, adapter, None)
            .await
            .expect("Server::new must succeed");

        // ── 4. Bind to ephemeral port ──────────────────────────────────────
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind to ephemeral port");
        let port = listener
            .local_addr()
            .expect("local_addr")
            .port();

        // ── 5. Spawn server as background task ─────────────────────────────
        let (tx, rx) = oneshot::channel::<()>();
        tokio::spawn(async move {
            server
                .serve_on_listener(listener, async {
                    let _ = rx.await; // intentional: drives graceful shutdown
                })
                .await
                .expect("server task must not fail");
        });

        // Give the Tokio accept loop time to start.
        tokio::time::sleep(Duration::from_millis(50)).await;

        Self {
            url:       format!("http://127.0.0.1:{port}"),
            _shutdown: tx,
            _fixtures: fixtures,
        }
    }
}

// ── The test ──────────────────────────────────────────────────────────────────

/// Full compile → PostgreSQL → HTTP round-trip.
///
/// Gated behind `#[ignore]` so it is skipped by default.  Enable with:
///
/// ```bash
/// FRAISEQL_PIPELINE_E2E=1 cargo test -p fraiseql-server \
///     --test pipeline_e2e_test -- --ignored
/// ```
#[tokio::test]
#[ignore = "requires Docker + FRAISEQL_PIPELINE_E2E=1"]
async fn pipeline_e2e_compile_to_http_query() {
    if std::env::var("FRAISEQL_PIPELINE_E2E").is_err() {
        eprintln!("Skipping pipeline_e2e_compile_to_http_query: FRAISEQL_PIPELINE_E2E not set");
        return;
    }

    // ── Fixture files ──────────────────────────────────────────────────────
    let fixtures = TempDir::new().expect("create temp dir");
    let toml_path = write_fixtures(&fixtures);

    // ── PostgreSQL via testcontainers ──────────────────────────────────────
    let pg = Postgres::default()
        .with_user("testuser")
        .with_password("testpw")
        .with_db_name("testdb")
        .start()
        .await
        .expect("start postgres testcontainer");
    let pg_port = pg.get_host_port_ipv4(5432).await.expect("postgres port");

    apply_ddl(pg_port).await;

    let db_url = format!("postgresql://testuser:testpw@127.0.0.1:{pg_port}/testdb");

    // ── Start server ───────────────────────────────────────────────────────
    let pipeline = TestPipeline::start(fixtures, &toml_path, &db_url).await;

    // ── Execute GraphQL query ──────────────────────────────────────────────
    let client = Client::new();
    let resp = client
        .post(format!("{}/graphql", pipeline.url))
        .json(&serde_json::json!({
            "query": "{ users { id name } }"
        }))
        .send()
        .await
        .expect("HTTP request must succeed")
        .json::<Value>()
        .await
        .expect("response must be valid JSON");

    // ── Assertions ─────────────────────────────────────────────────────────
    assert!(
        resp["errors"].is_null(),
        "expected no GraphQL errors, got: {}",
        resp["errors"]
    );

    let users = resp["data"]["users"]
        .as_array()
        .expect("data.users must be an array");

    assert_eq!(users.len(), 1, "expected exactly 1 user row, got: {resp}");

    let alice = &users[0];
    assert_eq!(alice["name"], "Alice", "full response: {resp}");
    assert!(
        alice["id"].as_i64().is_some(),
        "id must be an integer, full response: {resp}"
    );
}
