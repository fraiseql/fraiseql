//! Client API integration tests
//!
//! These tests require a running Postgres instance.

use fraiseql_wire::FraiseClient;
use futures::StreamExt;

#[tokio::test]
#[ignore] // Requires Postgres running
async fn test_client_connect() {
    let _client = FraiseClient::connect("postgres://postgres:postgres@localhost:5433/postgres")
        .await
        .expect("connect");

    // Note: FraiseClient consumes itself when building a query
    // To test connection, we just verify it connects successfully
}

#[tokio::test]
#[ignore] // Requires Postgres running
async fn test_client_query_streaming() {
    let client = FraiseClient::connect("postgres://postgres:postgres@localhost:5433/postgres")
        .await
        .expect("connect");

    // Build query (consumes client)
    let mut stream = client
        .query("test")
        .where_sql("1 = 1")
        .order_by("data->>'id' ASC")
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
}
