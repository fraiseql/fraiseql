#![allow(clippy::unwrap_used, clippy::panic)] // Reason: test code, panics acceptable
use super::*;
use crate::WireError;
use futures::{stream, StreamExt};

#[tokio::test]
async fn test_filter_stream() {
    let values = vec![
        Ok(serde_json::json!({"id": 1, "active": true})),
        Ok(serde_json::json!({"id": 2, "active": false})),
        Ok(serde_json::json!({"id": 3, "active": true})),
    ];

    let inner = stream::iter(values);

    let predicate: Predicate = Box::new(|v| v["active"].as_bool().unwrap_or(false));

    let mut filtered = FilteredStream::new(inner, predicate);

    let mut results = Vec::new();
    while let Some(item) = filtered.next().await {
        let value = item.unwrap();
        results.push(value["id"].as_i64().unwrap());
    }

    assert_eq!(results, vec![1, 3]);
}

#[tokio::test]
async fn test_filter_propagates_errors() {
    let values = vec![
        Ok(serde_json::json!({"id": 1})),
        Err(WireError::JsonDecode(serde_json::Error::io(
            std::io::Error::other("test error"),
        ))),
        Ok(serde_json::json!({"id": 2})),
    ];

    let inner = stream::iter(values);
    let predicate: Predicate = Box::new(|_| true);

    let mut filtered = FilteredStream::new(inner, predicate);

    // First item OK
    filtered
        .next()
        .await
        .unwrap()
        .unwrap_or_else(|e| panic!("expected Ok for first item, got: {e}"));

    // Second item is error
    let second = filtered.next().await.unwrap();
    assert!(
        matches!(second, Err(WireError::JsonDecode(_))),
        "expected JsonDecode error for second item, got: {second:?}"
    );

    // Third item OK
    filtered
        .next()
        .await
        .unwrap()
        .unwrap_or_else(|e| panic!("expected Ok for third item, got: {e}"));
}

#[tokio::test]
async fn test_filter_all_filtered_out() {
    let values = vec![
        Ok(serde_json::json!({"id": 1})),
        Ok(serde_json::json!({"id": 2})),
    ];

    let inner = stream::iter(values);
    let predicate: Predicate = Box::new(|_| false); // Filter everything

    let mut filtered = FilteredStream::new(inner, predicate);

    // Stream should be empty
    assert!(filtered.next().await.is_none());
}
