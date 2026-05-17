#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

use chrono::Duration;

use super::*;

#[tokio::test]
async fn test_cache_set_and_get() {
    let cache = SecretCache::new(10);
    let expiry = Utc::now() + Duration::hours(1);
    cache.set("db-creds".to_string(), "password123".to_string(), expiry).await;

    let result = cache.get_with_expiry("db-creds").await;
    assert!(result.is_some(), "cached secret must be returned before expiry");
    let (value, _) = result.unwrap();
    assert_eq!(value, "password123");
}

#[tokio::test]
async fn test_cache_returns_none_for_missing_key() {
    let cache = SecretCache::new(10);
    let result = cache.get_with_expiry("nonexistent").await;
    assert!(result.is_none(), "missing key must return None");
}

#[tokio::test]
async fn test_cache_returns_none_for_expired_entry() {
    let cache = SecretCache::new(10);
    // Set entry that expires in the past
    let expired = Utc::now() - Duration::seconds(1);
    cache.set("expired-key".to_string(), "stale".to_string(), expired).await;

    let result = cache.get_with_expiry("expired-key").await;
    assert!(result.is_none(), "expired entry must return None");
}

#[tokio::test]
async fn test_cache_invalidate_removes_entry() {
    let cache = SecretCache::new(10);
    let expiry = Utc::now() + Duration::hours(1);
    cache.set("to-remove".to_string(), "secret".to_string(), expiry).await;

    cache.invalidate("to-remove").await;

    let result = cache.get_with_expiry("to-remove").await;
    assert!(result.is_none(), "invalidated entry must return None");
}

#[tokio::test]
async fn test_cache_lru_eviction_at_capacity() {
    // Create a cache with max 5 entries
    let cache = SecretCache::new(5);
    let expiry = Utc::now() + Duration::hours(1);

    // Fill to capacity
    for i in 0..5 {
        cache.set(format!("key-{i}"), format!("val-{i}"), expiry).await;
    }

    // Adding a 6th entry should trigger LRU eviction of 10% = 1 entry
    cache.set("key-new".to_string(), "val-new".to_string(), expiry).await;

    // The new key must be present
    let result = cache.get_with_expiry("key-new").await;
    assert!(result.is_some(), "newly added entry must exist after eviction");
}

#[tokio::test]
async fn test_cache_overwrite_existing_key() {
    let cache = SecretCache::new(10);
    let expiry = Utc::now() + Duration::hours(1);
    cache.set("key".to_string(), "old-value".to_string(), expiry).await;
    cache.set("key".to_string(), "new-value".to_string(), expiry).await;

    let (value, _) = cache.get_with_expiry("key").await.expect("key must exist");
    assert_eq!(value, "new-value", "overwritten key must return new value");
}

#[test]
fn test_vault_response_deserializes() {
    let json = r#"{
        "request_id": "req-1",
        "lease_id": "lease-1",
        "lease_duration": 3600,
        "renewable": true,
        "data": {"username": "admin", "password": "secret"}
    }"#;
    let response: VaultResponse =
        serde_json::from_str(json).expect("VaultResponse must deserialize");
    assert_eq!(response.lease_duration, 3600);
    assert!(response.renewable);
    assert_eq!(response.data.get("username").and_then(|v| v.as_str()), Some("admin"));
}
