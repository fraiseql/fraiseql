//! Integration tests for JSON streaming
//!
//! These tests require a running Postgres instance.

use fraiseql_wire::connection::{Connection, ConnectionConfig, Transport};
use futures::StreamExt;

#[tokio::test]
#[ignore] // Requires Postgres running
async fn test_streaming_query() {
    let transport = Transport::connect_tcp("localhost", 5432)
        .await
        .expect("connect");

    let mut conn = Connection::new(transport);

    let config = ConnectionConfig::new("postgres", "postgres");
    conn.startup(&config).await.expect("startup");

    // Test with a simple JSON value
    let mut stream = conn
        .streaming_query("SELECT '{\"key\": \"value\"}'::json AS data", 10)
        .await
        .expect("query");

    let mut count = 0;
    while let Some(item) = stream.next().await {
        let value = item.expect("value");
        assert_eq!(value["key"], "value");
        count += 1;
    }

    assert_eq!(count, 1);
}
