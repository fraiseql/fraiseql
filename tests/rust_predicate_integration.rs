//! Integration tests for Rust predicates
//!
//! These tests require a running Postgres instance with a test view.

use fraiseql_wire::FraiseClient;
use futures::StreamExt;

#[tokio::test]
#[ignore] // Requires Postgres running
async fn test_hybrid_filtering() {
    let client = FraiseClient::connect("postgres://postgres:postgres@localhost:5433/postgres")
        .await
        .expect("connect");

    // This test assumes v_test view exists with JSON data like:
    // SELECT json_build_object('id', i, 'value', i * 10) AS data
    // FROM generate_series(1, 100) i

    let mut stream = client
        .query::<serde_json::Value>("test")
        .where_sql("(data->>'id')::int > 50")  // SQL: filter to id > 50
        .where_rust(|json| {
            // Rust: filter to even ids
            json["id"].as_i64().unwrap_or(0) % 2 == 0
        })
        .execute()
        .await
        .expect("query");

    let mut ids = Vec::new();
    while let Some(item) = stream.next().await {
        let json = item.expect("item");
        ids.push(json["id"].as_i64().unwrap());
    }

    // Should get: 52, 54, 56, ..., 100 (25 values)
    assert!(!ids.is_empty(), "should have results from hybrid filtering");
    assert!(ids[0] > 50, "all ids should be > 50");
    for id in &ids {
        assert_eq!(id % 2, 0, "all ids should be even");
    }
}
