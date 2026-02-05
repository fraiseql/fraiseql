//! Client API integration tests

mod common;

use common::connect_test_client;
use futures::StreamExt;

#[tokio::test]
async fn test_client_connect() {
    let _client = connect_test_client().await.expect("connect");

    // Note: FraiseClient consumes itself when building a query
    // To test connection, we just verify it connects successfully
}

#[tokio::test]
async fn test_client_query_streaming() {
    let client = connect_test_client().await.expect("connect");

    // Build query (consumes client)
    let mut stream = client
        .query::<serde_json::Value>("test.v_project")
        .where_sql("1 = 1")
        .order_by("data->>'name' ASC")
        .chunk_size(128)
        .execute()
        .await
        .expect("query");

    // Read from stream
    let mut count = 0;
    while let Some(result) = stream.next().await {
        let _json = result.expect("item");
        count += 1;
        // Limit iterations to avoid long-running tests
        if count > 10 {
            break;
        }
    }

    assert!(count > 0, "Should have streamed at least some results");
}
