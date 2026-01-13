//! Integration tests for fraiseql-wire
//!
//! These tests require a running Postgres instance.

use fraiseql_wire::connection::{Connection, ConnectionConfig, Transport};

#[tokio::test]
#[ignore] // Requires Postgres running
async fn test_connect_and_query() {
    let transport = Transport::connect_tcp("localhost", 5432)
        .await
        .expect("connect");

    let mut conn = Connection::new(transport);

    let config = ConnectionConfig::new("postgres", "postgres");
    conn.startup(&config).await.expect("startup");

    let messages = conn.simple_query("SELECT 1").await.expect("query");
    assert!(!messages.is_empty());

    conn.close().await.expect("close");
}
