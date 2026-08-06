//! Shared test infrastructure for fraiseql-wire integration tests.
//!
//! The backing PostgreSQL is provided by the test-support harness: a Dagger-bound
//! service in CI (DATABASE_URL injected) or a local testcontainer with the
//! `local-testcontainers` feature. The sample `test` schema and its seed come from
//! their single owner, `fraiseql_test_support::sample_schema` — `fraiseql-core`'s
//! suites read the same relations, and both crates used to provision them from
//! byte-identical private copies with contradictory seeding rules (#996). Both
//! scripts are idempotent, so every wire test binary can share one bound database
//! without duplicating rows.

#![allow(clippy::doc_markdown, clippy::print_stdout, clippy::print_stderr)] // Reason: test helper docs don't need backticks
#![allow(dead_code)] // Reason: shared helper compiled into every wire test binary; each binary uses a different subset of the fields/methods

use std::sync::Arc;

use tokio::sync::OnceCell;

use fraiseql_test_support::sample_schema::{SAMPLE_SCHEMA_SQL, SAMPLE_SEED_SQL};

/// Shared database handle for all tests in a test binary.
/// Using `OnceCell` ensures the schema/seed work happens only once.
static CONTAINER: OnceCell<Arc<TestContainer>> = OnceCell::const_new();

/// The harness-provided PostgreSQL plus its connection URL and parsed parts.
///
/// The parts (`host`/`port`/`user`/`password`/`database`) are exposed so tests that
/// build their own `Transport`/`ConnectionConfig` reach the service by its real host
/// (the Dagger bind alias in CI, `127.0.0.1` for a local spawn) rather than a
/// hardcoded `localhost`.
pub struct TestContainer {
    #[allow(dead_code)]
    // Reason: guard held alive so a locally-spawned container outlives the test binary
    service: fraiseql_test_support::Service,
    url: String,
    pub host: String,
    pub port: u16,
    pub user: String,
    pub password: String,
    pub database: String,
}

impl TestContainer {
    /// Get the connection string for this database.
    #[allow(dead_code)] // Reason: utility method used by subset of wire tests; Clippy false-positive (multi-binary)
    pub fn connection_string(&self) -> String {
        self.url.clone()
    }
}

/// Initialize the shared test database.
/// This is idempotent - calling it multiple times returns the same handle.
pub async fn get_test_container() -> Arc<TestContainer> {
    CONTAINER
        .get_or_init(|| async { Arc::new(provision_database().await) })
        .await
        .clone()
}

/// Provision the harness PostgreSQL: apply the schema (idempotent) and seed it once.
async fn provision_database() -> TestContainer {
    let service = fraiseql_test_support::postgres()
        .await
        .expect("DATABASE_URL must be set (or enable fraiseql-test-support/local-testcontainers)");
    let url = service.url().to_string();
    let parts = PgParts::parse(&url);

    // Initialize schema and seed data using tokio-postgres.
    let (client, connection) = tokio_postgres::connect(&url, tokio_postgres::NoTls)
        .await
        .expect("Failed to connect to harness Postgres for setup");

    // Spawn connection handler
    tokio::spawn(async move {
        if let Err(e) = connection.await {
            eprintln!("Connection error during setup: {}", e);
        }
    });

    // Both scripts are idempotent (schema: CREATE … IF NOT EXISTS / CREATE OR
    // REPLACE; seed: fixed ids + ON CONFLICT (id) DO NOTHING), so every binary and
    // every process can apply them unconditionally and the rows stay identical.
    client
        .batch_execute(SAMPLE_SCHEMA_SQL)
        .await
        .expect("Failed to create schema");
    client
        .batch_execute(SAMPLE_SEED_SQL)
        .await
        .expect("Failed to seed data");

    TestContainer {
        service,
        url,
        host: parts.host,
        port: parts.port,
        user: parts.user,
        password: parts.password,
        database: parts.database,
    }
}

/// Connection components parsed from a `postgres://`/`postgresql://` URL.
struct PgParts {
    host: String,
    port: u16,
    user: String,
    password: String,
    database: String,
}

impl PgParts {
    fn parse(url: &str) -> Self {
        let rest = url
            .strip_prefix("postgresql://")
            .or_else(|| url.strip_prefix("postgres://"))
            .expect("harness url must start with postgres://");
        let (userinfo, hostpart) = rest.split_once('@').expect("harness url must contain '@'");
        let (user, password) = userinfo.split_once(':').unwrap_or((userinfo, ""));
        let (hostport, dbpart) = hostpart.split_once('/').unwrap_or((hostpart, ""));
        let database = dbpart.split('?').next().unwrap_or("");
        let (host, port) = hostport.split_once(':').unwrap_or((hostport, "5432"));
        Self {
            host: host.to_string(),
            port: port.parse().unwrap_or(5432),
            user: user.to_string(),
            password: password.to_string(),
            database: database.to_string(),
        }
    }
}

/// Helper to connect a FraiseClient to the test database.
#[allow(dead_code)] // Reason: utility function used by subset of wire tests; Clippy false-positive (multi-binary)
pub async fn connect_test_client() -> fraiseql_wire::error::Result<fraiseql_wire::FraiseClient> {
    let container = get_test_container().await;
    fraiseql_wire::FraiseClient::connect(&container.connection_string()).await
}
