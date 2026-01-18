//! Integration tests for JSON streaming

mod common;

use common::get_test_container;
use fraiseql_wire::connection::{Connection, ConnectionConfig, Transport};
use futures::StreamExt;

#[tokio::test]
async fn test_streaming_query() {
    let container = get_test_container().await;

    let transport = Transport::connect_tcp("127.0.0.1", container.port)
        .await
        .expect("connect");

    let mut conn = Connection::new(transport);

    let config = ConnectionConfig::builder(&container.database, &container.user)
        .password(&container.password)
        .build();
    conn.startup(&config).await.expect("startup");

    // Test with a simple JSON value
    let mut stream = conn
        .streaming_query(
            "SELECT '{\"key\": \"value\"}'::json AS data",
            10,
            None,
            None,
            None,
            false,
            None,
            None,
        )
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
