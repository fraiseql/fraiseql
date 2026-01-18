//! Comprehensive integration tests
//!
//! Run with: cargo test --test integration_full -- --nocapture

mod common;

use common::connect_test_client;
use futures::StreamExt;

#[tokio::test]
async fn test_basic_connection() {
    let _client = connect_test_client().await.expect("connect");

    // Note: FraiseClient consumes self on use, so we just verify connection succeeds
}

#[tokio::test]
async fn test_simple_query() {
    let client = connect_test_client().await.expect("connect");

    let mut stream = client
        .query::<serde_json::Value>("test.v_project")
        .execute()
        .await
        .expect("query");

    let mut count = 0;
    while let Some(item) = stream.next().await {
        item.expect("item");
        count += 1;
    }

    assert!(count > 0, "Should have at least one project from seed data");
    println!("Received {} rows", count);
}

#[tokio::test]
async fn test_sql_predicate() {
    let client = connect_test_client().await.expect("connect");

    let mut stream = client
        .query::<serde_json::Value>("test.v_project")
        .where_sql("1 = 1")
        .execute()
        .await
        .expect("query");

    let mut count = 0;
    while let Some(item) = stream.next().await {
        item.expect("item");
        count += 1;
    }

    assert!(count > 0, "Should have at least one project");
}

#[tokio::test]
async fn test_rust_predicate() {
    let client = connect_test_client().await.expect("connect");

    let mut stream = client
        .query::<serde_json::Value>("test.v_project")
        .where_rust(|json| json.is_object())
        .execute()
        .await
        .expect("query");

    let mut count = 0;
    while let Some(item) = stream.next().await {
        let value = item.expect("item");
        assert!(value.is_object(), "Rust predicate should filter to objects");
        count += 1;
    }

    assert!(count > 0, "Should have at least one project");
}

#[tokio::test]
async fn test_order_by() {
    let client = connect_test_client().await.expect("connect");

    let mut stream = client
        .query::<serde_json::Value>("test.v_project")
        .order_by("data->>'name' ASC")
        .execute()
        .await
        .expect("query");

    let mut prev_name = String::new();
    while let Some(item) = stream.next().await {
        let value = item.expect("item");
        let name = value["name"].as_str().unwrap_or("").to_string();
        assert!(
            name >= prev_name,
            "Results should be ordered ascending by name: {} >= {}",
            name,
            prev_name
        );
        prev_name = name;
    }
}

#[tokio::test]
async fn test_chunk_size() {
    let client = connect_test_client().await.expect("connect");

    let mut stream = client
        .query::<serde_json::Value>("test.v_project")
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
    assert!(count > 0, "Should have at least one project");
}

#[tokio::test]
async fn test_multiple_predicates() {
    let client = connect_test_client().await.expect("connect");

    let mut stream = client
        .query::<serde_json::Value>("test.v_project")
        .where_sql("1 = 1")
        .where_sql("1 = 1")
        .where_rust(|v| v.is_object())
        .execute()
        .await
        .expect("query");

    let mut count = 0;
    while let Some(item) = stream.next().await {
        let value = item.expect("item");
        assert!(value.is_object());
        count += 1;
    }

    assert!(count > 0, "Should have at least one project");
}

#[tokio::test]
async fn test_connection_string_tcp() {
    // Test TCP connection via the shared container
    let _client = connect_test_client().await.expect("connect");
}

#[tokio::test]
async fn test_error_handling_invalid_view() {
    let client = connect_test_client().await.expect("connect");

    let result = client
        .query::<serde_json::Value>("nonexistent_view")
        .execute()
        .await;

    // Query execution fails against invalid view
    assert!(result.is_err(), "Query against non-existent view should fail");
    println!("Invalid view result: error as expected");
}

#[tokio::test]
async fn test_empty_result_set() {
    let client = connect_test_client().await.expect("connect");

    let mut stream = client
        .query::<serde_json::Value>("test.v_project")
        .where_sql("FALSE") // Filter all rows
        .execute()
        .await
        .expect("query");

    let mut count = 0;
    while let Some(item) = stream.next().await {
        item.expect("item");
        count += 1;
    }

    assert_eq!(count, 0, "Empty result set should have 0 rows");
    println!("Empty result set test: {} rows", count);
}
