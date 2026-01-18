//! Shared test infrastructure for fraiseql-wire integration tests.
//!
//! Provides a PostgreSQL testcontainer with the test schema and seed data.

use std::sync::Arc;
use testcontainers_modules::{
    postgres::Postgres,
    testcontainers::{runners::AsyncRunner, ContainerAsync, ImageExt},
};
use tokio::sync::OnceCell;

/// Schema SQL to create test tables and views
const SCHEMA_SQL: &str = include_str!("../fixtures/schema.sql");

/// Seed data SQL to populate test tables
const SEED_SQL: &str = include_str!("../fixtures/seed_data.sql");

/// Shared container instance for all tests in a test binary.
/// Using OnceCell ensures the container is only started once.
static CONTAINER: OnceCell<Arc<TestContainer>> = OnceCell::const_new();

/// Wrapper around the PostgreSQL container with connection info.
pub struct TestContainer {
    #[allow(dead_code)]
    container: ContainerAsync<Postgres>,
    pub port: u16,
    pub user: String,
    pub password: String,
    pub database: String,
}

impl TestContainer {
    /// Get the connection string for this container.
    #[allow(dead_code)]
    pub fn connection_string(&self) -> String {
        format!(
            "postgres://{}:{}@127.0.0.1:{}/{}",
            self.user, self.password, self.port, self.database
        )
    }
}

/// Initialize the shared test container.
/// This is idempotent - calling it multiple times returns the same container.
pub async fn get_test_container() -> Arc<TestContainer> {
    CONTAINER
        .get_or_init(|| async {
            let container = start_postgres_container().await;
            Arc::new(container)
        })
        .await
        .clone()
}

/// Start a new PostgreSQL container with schema and seed data.
async fn start_postgres_container() -> TestContainer {
    let user = "testuser";
    let password = "testpassword";
    let database = "testdb";

    // Start PostgreSQL container with SCRAM-SHA-256 authentication
    // (fraiseql-wire doesn't support MD5 authentication)
    let container = Postgres::default()
        .with_user(user)
        .with_password(password)
        .with_db_name(database)
        .with_env_var("POSTGRES_HOST_AUTH_METHOD", "scram-sha-256")
        .with_env_var("POSTGRES_INITDB_ARGS", "--auth-host=scram-sha-256")
        .start()
        .await
        .expect("Failed to start PostgreSQL container");

    let port = container
        .get_host_port_ipv4(5432)
        .await
        .expect("Failed to get container port");

    // Initialize schema and seed data using tokio-postgres
    let conn_string = format!(
        "host=127.0.0.1 port={} user={} password={} dbname={}",
        port, user, password, database
    );

    let (client, connection) = tokio_postgres::connect(&conn_string, tokio_postgres::NoTls)
        .await
        .expect("Failed to connect to container for setup");

    // Spawn connection handler
    tokio::spawn(async move {
        if let Err(e) = connection.await {
            eprintln!("Connection error during setup: {}", e);
        }
    });

    // Run schema SQL
    client
        .batch_execute(SCHEMA_SQL)
        .await
        .expect("Failed to create schema");

    // Run seed SQL
    client
        .batch_execute(SEED_SQL)
        .await
        .expect("Failed to seed data");

    TestContainer {
        container,
        port,
        user: user.to_string(),
        password: password.to_string(),
        database: database.to_string(),
    }
}

/// Helper to connect a FraiseClient to the test container.
#[allow(dead_code)]
pub async fn connect_test_client() -> fraiseql_wire::error::Result<fraiseql_wire::FraiseClient> {
    let container = get_test_container().await;
    let conn_string = container.connection_string();
    fraiseql_wire::FraiseClient::connect(&conn_string).await
}
