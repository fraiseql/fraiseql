//! Tests for the `idempotency` module.

#![allow(clippy::unwrap_used)]

use serde_json::json;

use super::*;

fn make_store(ttl_secs: u64) -> InMemoryIdempotencyStore {
    InMemoryIdempotencyStore::new(Duration::from_secs(ttl_secs), 100)
}

fn make_response() -> StoredResponse {
    StoredResponse {
        status:  201,
        headers: vec![("x-request-id".to_string(), "abc".to_string())],
        body:    Some(json!({"id": 1, "name": "Alice"})),
    }
}

#[tokio::test]
async fn new_key_returns_new() {
    let store = make_store(3600);
    let body_hash = hash_body(&json!({"name": "Alice"}));
    assert!(matches!(store.check("key1", body_hash).await, IdempotencyCheck::New));
}

#[tokio::test]
async fn stored_key_replays_response() {
    let store = make_store(3600);
    let body = json!({"name": "Alice"});
    let body_hash = hash_body(&body);
    let response = make_response();

    store.store("key1".to_string(), body_hash, response).await;

    match store.check("key1", body_hash).await {
        IdempotencyCheck::Replay(stored) => {
            assert_eq!(stored.status, 201);
            assert_eq!(stored.body.as_ref().unwrap()["name"], "Alice");
        },
        other => panic!("Expected Replay, got {other:?}"),
    }
}

#[tokio::test]
async fn same_key_different_body_returns_conflict() {
    let store = make_store(3600);
    let body1 = json!({"name": "Alice"});
    let body2 = json!({"name": "Bob"});
    let hash1 = hash_body(&body1);
    let hash2 = hash_body(&body2);

    store.store("key1".to_string(), hash1, make_response()).await;

    assert!(matches!(store.check("key1", hash2).await, IdempotencyCheck::Conflict));
}

#[tokio::test]
async fn expired_key_treated_as_new() {
    let store = InMemoryIdempotencyStore::new(Duration::from_millis(1), 100);
    let body = json!({"name": "Alice"});
    let body_hash = hash_body(&body);

    store.store("key1".to_string(), body_hash, make_response()).await;

    // Wait for TTL to expire
    tokio::time::sleep(Duration::from_millis(5)).await;

    assert!(matches!(store.check("key1", body_hash).await, IdempotencyCheck::New));
}

#[tokio::test]
async fn max_entries_evicts_oldest() {
    let store = InMemoryIdempotencyStore::new(Duration::from_secs(3600), 3);
    let hash = hash_body(&json!({}));

    store.store("key1".to_string(), hash, make_response()).await;
    tokio::time::sleep(Duration::from_millis(1)).await;
    store.store("key2".to_string(), hash, make_response()).await;
    tokio::time::sleep(Duration::from_millis(1)).await;
    store.store("key3".to_string(), hash, make_response()).await;
    tokio::time::sleep(Duration::from_millis(1)).await;

    // This should evict key1 (oldest)
    store.store("key4".to_string(), hash, make_response()).await;

    assert!(matches!(store.check("key1", hash).await, IdempotencyCheck::New));
    // key2 should still be there
    assert!(matches!(store.check("key2", hash).await, IdempotencyCheck::Replay(_)));
}

#[test]
fn body_hash_deterministic() {
    let body = json!({"name": "Alice", "age": 30});
    let hash1 = hash_body(&body);
    let hash2 = hash_body(&body);
    assert_eq!(hash1, hash2);
}

#[test]
fn body_hash_different_for_different_bodies() {
    let hash1 = hash_body(&json!({"name": "Alice"}));
    let hash2 = hash_body(&json!({"name": "Bob"}));
    assert_ne!(hash1, hash2);
}

#[tokio::test]
async fn create_store_returns_arc() {
    let store = create_store(3600);
    let body_hash = hash_body(&json!({}));
    assert!(matches!(store.check("key1", body_hash).await, IdempotencyCheck::New));
}
