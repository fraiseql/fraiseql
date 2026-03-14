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
#[allow(dead_code)]
pub(super) struct VaultResponse {
    pub(super) request_id:     String,
    pub(super) lease_id:       String,
    pub(super) lease_duration: i64,
    pub(super) renewable:      bool,
    pub(super) data:           HashMap<String, serde_json::Value>,
}

/// Cached secret with expiry metadata and LRU tracking.
///
/// `value` is wrapped in [`Zeroizing`] so that the secret bytes are overwritten
/// on drop rather than lingering in heap until the allocator reuses the memory.
#[derive(Debug, Clone)]
struct CachedSecret {
    value:         Zeroizing<String>,
    expires_at:    chrono::DateTime<Utc>,
    /// Last access time, used for LRU eviction ordering.
    last_accessed: chrono::DateTime<Utc>,
}

/// Secret cache with TTL management and LRU eviction for credential caching.
#[derive(Debug)]
pub(super) struct SecretCache {
    entries:     Arc<RwLock<HashMap<String, CachedSecret>>>,
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
