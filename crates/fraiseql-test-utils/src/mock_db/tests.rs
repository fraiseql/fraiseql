#![allow(clippy::panic)] // Reason: test code, panics acceptable
use serde_json::json;

use super::*;

#[tokio::test]
async fn test_mock_db_insert_and_get() {
    let db = MockDb::new();
    db.insert("user_1".to_string(), json!({"id": "1", "name": "Alice"})).await;

    let value = db
        .get("user_1")
        .await
        .unwrap_or_else(|e| panic!("expected Ok from mock_db.get: {e:?}"));
    assert_eq!(value["name"], "Alice");
}

#[tokio::test]
async fn test_mock_db_get_not_found() {
    let db = MockDb::new();
    let result = db.get("nonexistent").await;
    assert_eq!(result.unwrap_err(), MockDbError::NotFound);
}

#[tokio::test]
async fn test_mock_db_exists() {
    let db = MockDb::new();
    db.insert("key1".to_string(), json!({"value": 1})).await;

    assert!(db.exists("key1").await);
    assert!(!db.exists("key2").await);
}

#[tokio::test]
async fn test_mock_db_keys() {
    let db = MockDb::new();
    db.insert("key1".to_string(), json!({})).await;
    db.insert("key2".to_string(), json!({})).await;

    let keys = db.keys().await;
    assert_eq!(keys.len(), 2);
    assert!(keys.contains(&"key1".to_string()));
    assert!(keys.contains(&"key2".to_string()));
}

#[tokio::test]
async fn test_mock_db_clear() {
    let db = MockDb::new();
    db.insert("key1".to_string(), json!({})).await;
    db.insert("key2".to_string(), json!({})).await;

    db.clear().await;
    let keys = db.keys().await;
    assert_eq!(keys.len(), 0);
}
