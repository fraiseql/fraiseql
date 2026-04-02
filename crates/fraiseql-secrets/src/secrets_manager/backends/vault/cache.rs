use std::{collections::HashMap, sync::Arc};

use chrono::Utc;
use tokio::sync::RwLock;
use zeroize::Zeroizing;

/// Fraction of lease duration used as the cache TTL.
/// Caching until 80% of the credential lease avoids returning stale data.
pub(super) const CACHE_TTL_PERCENTAGE: f64 = 0.8;

/// Upper bound on cached entries. Excess entries are evicted LRU-style.
pub(super) const DEFAULT_MAX_CACHE_ENTRIES: usize = 1_000;

/// Vault API response structure for secrets.
#[derive(Debug, Clone, serde::Deserialize)]
// Reason: fields populated by serde deserialization; only `data` and
// `lease_duration` are accessed in business logic; the rest are kept for
// completeness and potential future auditing.
#[allow(dead_code)] // Reason: field kept for API completeness; may be used in future features
pub(super) struct VaultResponse {
    pub(super) request_id: String,
    pub(super) lease_id: String,
    pub(super) lease_duration: i64,
    pub(super) renewable: bool,
    pub(super) data: HashMap<String, serde_json::Value>,
}

/// Cached secret with expiry metadata and LRU tracking.
///
/// `value` is wrapped in [`Zeroizing`] so that the secret bytes are overwritten
/// on drop rather than lingering in heap until the allocator reuses the memory.
#[derive(Debug, Clone)]
struct CachedSecret {
    value: Zeroizing<String>,
    expires_at: chrono::DateTime<Utc>,
    /// Last access time, used for LRU eviction ordering.
    last_accessed: chrono::DateTime<Utc>,
}

/// Secret cache with TTL management and LRU eviction for credential caching.
#[derive(Debug)]
pub(super) struct SecretCache {
    entries: Arc<RwLock<HashMap<String, CachedSecret>>>,
    max_entries: usize,
}

impl SecretCache {
    /// Create new secret cache with specified max entries.
    pub(super) fn new(max_entries: usize) -> Self {
        SecretCache {
            entries: Arc::new(RwLock::new(HashMap::new())),
            max_entries,
        }
    }

    /// Get cached secret with expiry information, updating last-access time for LRU.
    pub(super) async fn get_with_expiry(
        &self,
        key: &str,
    ) -> Option<(String, chrono::DateTime<Utc>)> {
        let mut entries = self.entries.write().await;
        if let Some(cached) = entries.get_mut(key) {
            if cached.expires_at > Utc::now() {
                cached.last_accessed = Utc::now();
                return Some(((*cached.value).clone(), cached.expires_at));
            }
        }
        None
    }

    /// Remove a cached entry, forcing the next read to fetch fresh from Vault.
    pub(super) async fn invalidate(&self, key: &str) {
        self.entries.write().await.remove(key);
    }

    /// Store secret in cache with expiry.
    ///
    /// The secret is wrapped in [`Zeroizing`] on insertion so the bytes are
    /// overwritten when the entry is evicted or the cache is dropped.
    pub(super) async fn set(&self, key: String, secret: String, expires_at: chrono::DateTime<Utc>) {
        let mut entries = self.entries.write().await;

        // LRU eviction: if at capacity, remove the least-recently-accessed 10% of entries.
        if entries.len() >= self.max_entries {
            let remove_count = (self.max_entries / 10).max(1);
            let mut by_access: Vec<_> =
                entries.iter().map(|(k, v)| (k.clone(), v.last_accessed)).collect();
            by_access.sort_by_key(|(_, accessed)| *accessed);
            for (key, _) in by_access.into_iter().take(remove_count) {
                entries.remove(&key);
            }
        }

        let now = Utc::now();
        entries.insert(
            key,
            CachedSecret {
                value: Zeroizing::new(secret),
                expires_at,
                last_accessed: now,
            },
        );
    }
}

#[cfg(test)]
mod tests {
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
}
