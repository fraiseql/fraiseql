//! Testcontainer infrastructure for integration tests.
//!
//! Provides a shared PostgreSQL container and a connected `PostgresAdapter`.
#![allow(dead_code)]

use std::sync::Arc;

use fraiseql_core::db::postgres::PostgresAdapter;
use testcontainers_modules::{
    postgres::Postgres,
    testcontainers::{ContainerAsync, runners::AsyncRunner},
};
use tokio::sync::OnceCell;

const SCHEMA_SQL: &str = include_str!("../fixtures/schema.sql");
const SEED_SQL: &str = include_str!("../fixtures/seed_data.sql");

static CONTAINER: OnceCell<Arc<TestContainer>> = OnceCell::const_new();

pub struct TestContainer {
    #[allow(dead_code)]
    // Reason: container held alive to keep Docker container running for test duration
    container: ContainerAsync<Postgres>,
    pub port:     u16,
    pub user:     String,
    pub password: String,
    pub database: String,
}

impl TestContainer {
    pub fn connection_string(&self) -> String {
        format!(
            "postgres://{}:{}@127.0.0.1:{}/{}",
            self.user, self.password, self.port, self.database
        )
    }
}

pub async fn get_test_container() -> Arc<TestContainer> {
    CONTAINER
        .get_or_init(|| async {
            let container = start_postgres_container().await;
            Arc::new(container)
        })
        .await
        .clone()
}

async fn start_postgres_container() -> TestContainer {
    let user = "testuser";
    let password = "testpassword";
    let database = "testdb";

    let container = Postgres::default()
        .with_user(user)
        .with_password(password)
        .with_db_name(database)
        .start()
        .await
        .expect("Failed to start PostgreSQL container");

    let port = container.get_host_port_ipv4(5432).await.expect("Failed to get container port");

    let conn_string =
        format!("host=127.0.0.1 port={port} user={user} password={password} dbname={database}",);

    let (client, connection) = tokio_postgres::connect(&conn_string, tokio_postgres::NoTls)
        .await
        .expect("Failed to connect to container for setup");

    tokio::spawn(async move {
        if let Err(e) = connection.await {
            eprintln!("Connection error during setup: {e}");
        }
    });

    client.batch_execute(SCHEMA_SQL).await.expect("Failed to create schema");

    client.batch_execute(SEED_SQL).await.expect("Failed to seed data");

    TestContainer {
        container,
        port,
        user: user.to_string(),
        password: password.to_string(),
        database: database.to_string(),
    }
}

/// Get a `PostgresAdapter` connected to the shared test container.
pub async fn get_test_adapter() -> PostgresAdapter {
    let container = get_test_container().await;
    PostgresAdapter::new(&container.connection_string())
        .await
        .expect("Failed to create test adapter")
}
