//! Integration tests for fraiseql-wire

mod common;

use common::get_test_container;
use fraiseql_wire::connection::{Connection, ConnectionConfig, Transport};

#[tokio::test]
async fn test_connect_and_query() {
    let container = get_test_container().await;

    let transport = Transport::connect_tcp("127.0.0.1", container.port)
        .await
        .expect("connect");

    let mut conn = Connection::new(transport);

    let config = ConnectionConfig::builder(&container.database, &container.user)
        .password(&container.password)
        .build();
    conn.startup(&config).await.expect("startup");

    let messages = conn.simple_query("SELECT 1").await.expect("query");
    assert!(!messages.is_empty());

    conn.close().await.expect("close");
}
