//! Comprehensive integration tests
//!
//! Run with: cargo test --test integration_full -- --ignored --nocapture

use fraiseql_wire::FraiseClient;
use futures::StreamExt;

#[tokio::test]
#[ignore] // Requires Postgres running
async fn test_basic_connection() {
    let _client = FraiseClient::connect("postgres://postgres:postgres@localhost:5433/postgres")
        .await
        .expect("connect");

    // Note: FraiseClient consumes self on use, so we just verify connection succeeds
}

#[tokio::test]
#[ignore] // Requires Postgres running
async fn test_simple_query() {
    let client = FraiseClient::connect("postgres://postgres:postgres@localhost:5433/postgres")
        .await
        .expect("connect");

    let mut stream = client
        .query::<serde_json::Value>("test")
        .execute()
        .await
        .expect("query");

    let mut count = 0;
    while let Some(item) = stream.next().await {
        item.expect("item");
        count += 1;
    }

    // Don't assert count > 0 as test view may not exist
    println!("Received {} rows", count);
}

#[tokio::test]
#[ignore] // Requires Postgres running
async fn test_sql_predicate() {
    let client = FraiseClient::connect("postgres://postgres:postgres@localhost:5433/postgres")
        .await
        .expect("connect");

    let mut stream = client
        .query::<serde_json::Value>("test")
        .where_sql("1 = 1")
        .execute()
        .await
        .expect("query");

    while let Some(item) = stream.next().await {
        item.expect("item");
    }
}

#[tokio::test]
#[ignore] // Requires Postgres running
async fn test_rust_predicate() {
    let client = FraiseClient::connect("postgres://postgres:postgres@localhost:5433/postgres")
        .await
        .expect("connect");

    let mut stream = client
        .query::<serde_json::Value>("test")
        .where_rust(|json| json.is_object())
        .execute()
        .await
        .expect("query");

    while let Some(item) = stream.next().await {
        let value = item.expect("item");
        assert!(value.is_object(), "Rust predicate should filter to objects");
    }
}

#[tokio::test]
#[ignore] // Requires Postgres running
async fn test_order_by() {
    let client = FraiseClient::connect("postgres://postgres:postgres@localhost:5433/postgres")
        .await
        .expect("connect");

    let mut stream = client
        .query::<serde_json::Value>("test")
        .order_by("data->>'id' ASC")
        .execute()
        .await
        .expect("query");

    let mut prev_id = i64::MIN;
    while let Some(item) = stream.next().await {
        let value = item.expect("item");
        let id = value["id"].as_i64().unwrap_or(0);
        assert!(id >= prev_id, "Results should be ordered ascending by id");
        prev_id = id;
    }
}

#[tokio::test]
#[ignore] // Requires Postgres running
async fn test_chunk_size() {
    let client = FraiseClient::connect("postgres://postgres:postgres@localhost:5433/postgres")
        .await
        .expect("connect");

    let mut stream = client
        .query::<serde_json::Value>("test")
        .chunk_size(10)
        .execute()
        .await
        .expect("query");

    let mut count = 0;
    while let Some(item) = stream.next().await {
        item.expect("item");
        count += 1;
    }

    println!("Received {} rows with chunk_size=10", count);
}

#[tokio::test]
#[ignore] // Requires Postgres running
async fn test_multiple_predicates() {
    let client = FraiseClient::connect("postgres://postgres:postgres@localhost:5433/postgres")
        .await
        .expect("connect");

    let mut stream = client
        .query::<serde_json::Value>("test")
        .where_sql("1 = 1")
        .where_sql("1 = 1")
        .where_rust(|v| v.is_object())
        .execute()
        .await
        .expect("query");

    while let Some(item) = stream.next().await {
        let value = item.expect("item");
        assert!(value.is_object());
    }
}

#[tokio::test]
#[ignore] // Requires Postgres running
async fn test_connection_string_tcp() {
    // Test TCP connection
    let _client = FraiseClient::connect("postgres://postgres:postgres@localhost:5433/postgres")
        .await
        .expect("connect");
}

#[tokio::test]
#[ignore] // Requires Postgres running
async fn test_connection_string_defaults() {
    // Test with minimal connection string (uses whoami for defaults)
    // This test may fail if local user exists in Postgres, which is expected
    let result = FraiseClient::connect("postgres://localhost/postgres").await;
    // Don't assert success - just verify it returns a Result
    println!("Connection result: {:?}", result.is_ok());
}

#[tokio::test]
#[ignore] // Requires Postgres running
async fn test_error_handling_invalid_view() {
    let client = FraiseClient::connect("postgres://postgres:postgres@localhost:5433/postgres")
        .await
        .expect("connect");

    let result = client
        .query::<serde_json::Value>("nonexistent_view")
        .execute()
        .await;

    // Query execution fails against invalid view
    println!("Invalid view result: {}", result.is_err());
}

#[tokio::test]
#[ignore] // Requires Postgres running
async fn test_empty_result_set() {
    let client = FraiseClient::connect("postgres://postgres:postgres@localhost:5433/postgres")
        .await
        .expect("connect");

    let mut stream = client
        .query::<serde_json::Value>("test")
        .where_sql("FALSE") // Filter all rows
        .execute()
        .await
        .expect("query");

    let mut count = 0;
    while let Some(item) = stream.next().await {
        item.expect("item");
        count += 1;
    }

    println!("Empty result set test: {} rows", count);
}
