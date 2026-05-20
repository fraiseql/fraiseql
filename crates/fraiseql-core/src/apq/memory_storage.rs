//! In-memory APQ storage backend with LRU eviction and TTL support.

use std::{collections::HashMap, time::Duration};

use tokio::time::Instant;

use async_trait::async_trait;
use serde_json::json;

use super::storage::{ApqError, ApqStats, ApqStorage};

/// Default TTL for stored queries (1 hour).
const DEFAULT_TTL: Duration = Duration::from_secs(3600);

/// Default maximum number of stored queries.
const DEFAULT_MAX_ENTRIES: usize = 1000;

/// A stored query with metadata for TTL and LRU tracking.
struct StoredQuery {
    body: String,
    stored_at: Instant,
    ttl: Duration,
    last_accessed: Instant,
}

impl StoredQuery {
    fn is_expired(&self) -> bool {
        self.stored_at.elapsed() >= self.ttl
    }
}

/// In-memory APQ storage with LRU eviction and TTL expiry.
///
/// Stores persisted queries in a `HashMap` with configurable capacity
/// and time-to-live. When capacity is reached, expired entries are
/// purged first, then the least-recently-accessed entry is evicted.
pub struct InMemoryApqStorage {
    entries: tokio::sync::Mutex<HashMap<String, StoredQuery>>,
    max_entries: usize,
    ttl: Duration,
}

impl InMemoryApqStorage {
    /// Create a new store with the given maximum entry count and default TTL.
    #[must_use]
    pub fn new(max_entries: usize) -> Self {
        Self {
            entries: tokio::sync::Mutex::new(HashMap::new()),
            max_entries,
            ttl: DEFAULT_TTL,
        }
    }

    /// Create a new store with custom capacity and TTL.
    #[must_use]
    pub fn with_ttl(max_entries: usize, ttl: Duration) -> Self {
        Self {
            entries: tokio::sync::Mutex::new(HashMap::new()),
            max_entries,
            ttl,
        }
    }
}

impl Default for InMemoryApqStorage {
    fn default() -> Self {
        Self::new(DEFAULT_MAX_ENTRIES)
    }
}

// Reason: ApqStorage is defined with #[async_trait]; all implementations must match
// its transformed method signatures to satisfy the trait contract
// async_trait: dyn-dispatch required; remove when RTN + Send is stable (RFC 3425)
#[async_trait]
impl ApqStorage for InMemoryApqStorage {
    async fn get(&self, hash: &str) -> Result<Option<String>, ApqError> {
        let mut map = self.entries.lock().await;

        // Check if the entry exists and is not expired.
        if let Some(entry) = map.get_mut(hash) {
            if entry.is_expired() {
                map.remove(hash);
                return Ok(None);
            }
            entry.last_accessed = Instant::now();
            return Ok(Some(entry.body.clone()));
        }

        Ok(None)
    }

    async fn set(&self, hash: String, query: String) -> Result<(), ApqError> {
        let mut map = self.entries.lock().await;

        let now = Instant::now();

        // Purge expired entries first.
        map.retain(|_, v| !v.is_expired());

        // If still at capacity, evict the least-recently-accessed entry.
        if map.len() >= self.max_entries && !map.contains_key(&hash) {
            if let Some(lru_key) =
                map.iter().min_by_key(|(_, v)| v.last_accessed).map(|(k, _)| k.clone())
            {
                map.remove(&lru_key);
            }
        }

        map.insert(
            hash,
            StoredQuery {
                body: query,
                stored_at: now,
                ttl: self.ttl,
                last_accessed: now,
            },
        );

        Ok(())
    }

    async fn exists(&self, hash: &str) -> Result<bool, ApqError> {
        let mut map = self.entries.lock().await;

        if let Some(entry) = map.get(hash) {
            if entry.is_expired() {
                map.remove(hash);
                return Ok(false);
            }
            return Ok(true);
        }

        Ok(false)
    }

    async fn remove(&self, hash: &str) -> Result<(), ApqError> {
        let mut map = self.entries.lock().await;
        map.remove(hash);
        Ok(())
    }

    async fn stats(&self) -> Result<ApqStats, ApqError> {
        let map = self.entries.lock().await;

        let total = map.len();
        let expired = map.values().filter(|v| v.is_expired()).count();

        Ok(ApqStats::with_extra(
            total,
            "memory".to_string(),
            json!({
                "max_entries": self.max_entries,
                "ttl_secs": self.ttl.as_secs(),
                "expired_pending_cleanup": expired,
            }),
        ))
    }

    async fn clear(&self) -> Result<(), ApqError> {
        let mut map = self.entries.lock().await;
        map.clear();
        Ok(())
    }
}
