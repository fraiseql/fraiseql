//! Shared test database for integration tests.
//!
//! Provides a connected `PostgresAdapter` against the harness-provided Postgres
//! (Dagger-bound in CI; a local spawn with the `local-testcontainers` feature). The
//! fixtures create an isolated `test` schema (`CREATE SCHEMA IF NOT EXISTS` + per-table
//! `TRUNCATE … CASCADE`, seeds use `ON CONFLICT DO NOTHING`), so a shared database is
//! fine — no per-test isolation is required.
#![allow(dead_code)]
#![allow(clippy::print_stdout, clippy::print_stderr)] // Reason: CLI / test / example / bench code prints to stdout/stderr by design
use std::sync::Arc;

use fraiseql_core::db::postgres::PostgresAdapter;
use tokio::sync::OnceCell;

const SCHEMA_SQL: &str = include_str!("../fixtures/schema.sql");
const SEED_SQL: &str = include_str!("../fixtures/seed_data.sql");

static CONTAINER: OnceCell<Arc<TestContainer>> = OnceCell::const_new();

pub struct TestContainer {
    // Held only to keep a locally-spawned container alive for the test's duration
    // (no-op when the URL comes from the environment).
    #[allow(dead_code)]
    service: fraiseql_test_support::Service,
}

impl TestContainer {
    pub fn connection_string(&self) -> String {
        self.service.url().to_string()
    }
}

pub async fn get_test_container() -> Arc<TestContainer> {
    CONTAINER
        .get_or_init(|| async { Arc::new(start_postgres().await) })
        .await
        .clone()
}

async fn start_postgres() -> TestContainer {
    let service = fraiseql_test_support::postgres().await.expect(
        "DATABASE_URL must be set (e.g. via `dagger call test-integration`) or enable the \
         fraiseql-test-support/local-testcontainers feature",
    );

    let (client, connection) = tokio_postgres::connect(service.url(), tokio_postgres::NoTls)
        .await
        .expect("Failed to connect to test database for setup");
    tokio::spawn(async move {
        if let Err(e) = connection.await {
            eprintln!("Connection error during setup: {e}");
        }
    });

    client.batch_execute(SCHEMA_SQL).await.expect("Failed to create schema");
    client.batch_execute(SEED_SQL).await.expect("Failed to seed data");

    TestContainer { service }
}

/// Get a `PostgresAdapter` connected to the shared test database.
pub async fn get_test_adapter() -> PostgresAdapter {
    let container = get_test_container().await;
    PostgresAdapter::new(&container.connection_string())
        .await
        .expect("Failed to create test adapter")
}
