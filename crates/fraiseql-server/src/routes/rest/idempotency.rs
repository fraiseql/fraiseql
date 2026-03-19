//! Idempotency key support for REST POST mutations.
//!
//! Clients include an `Idempotency-Key` header on POST requests.  If a response
//! for that key has already been stored, it is replayed.  If the same key is
//! reused with a different request body, a 422 IDEMPOTENCY_CONFLICT error is
//! returned.
//!
//! GET, PUT, and DELETE are inherently idempotent — the key is ignored for those
//! methods.
//!
//! The default [`InMemoryIdempotencyStore`] uses a [`DashMap`] with TTL-based
//! expiry.  A Redis-backed implementation can be added under a feature flag.

use std::sync::Arc;
use std::time::{Duration, Instant};

use dashmap::DashMap;
use serde_json::Value;
use xxhash_rust::xxh3::xxh3_64;

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// Result of checking an idempotency key.
#[derive(Debug)]
pub enum IdempotencyCheck {
    /// No previous request with this key — proceed with execution.
    New,
    /// Previous request found with matching body — replay the stored response.
    Replay(StoredResponse),
    /// Previous request found with DIFFERENT body — return 422.
    Conflict,
}

/// A stored response for idempotency replay.
#[derive(Debug, Clone)]
pub struct StoredResponse {
    /// HTTP status code.
    pub status: u16,
    /// Response headers (key, value) pairs.
    pub headers: Vec<(String, String)>,
    /// Response body (if any).
    pub body: Option<Value>,
}

/// Entry in the idempotency store.
struct Entry {
    response: StoredResponse,
    body_hash: u64,
    created_at: Instant,
}

// ---------------------------------------------------------------------------
// In-memory store
// ---------------------------------------------------------------------------

/// In-memory idempotency store backed by [`DashMap`].
///
/// Entries expire after the configured TTL.  Expired entries are lazily evicted
/// on access and periodically during insertions.
pub struct InMemoryIdempotencyStore {
    entries: DashMap<String, Entry>,
    ttl: Duration,
    max_entries: usize,
}

impl InMemoryIdempotencyStore {
    /// Create a new in-memory idempotency store.
    #[must_use]
    pub fn new(ttl: Duration, max_entries: usize) -> Self {
        Self {
            entries: DashMap::new(),
            ttl,
            max_entries,
        }
    }

    /// Hash a request body for conflict detection.
    #[must_use]
    pub fn hash_body(body: &Value) -> u64 {
        let bytes = serde_json::to_vec(body).unwrap_or_default();
        xxh3_64(&bytes)
    }

    /// Check an idempotency key against the store.
    ///
    /// Returns [`IdempotencyCheck::New`] if no entry exists (or it has expired),
    /// [`IdempotencyCheck::Replay`] if the key matches with the same body hash,
    /// or [`IdempotencyCheck::Conflict`] if the key matches with a different body.
    pub fn check(&self, key: &str, body_hash: u64) -> IdempotencyCheck {
        if let Some(entry) = self.entries.get(key) {
            if entry.created_at.elapsed() > self.ttl {
                // Expired — treat as new (drop expired entry)
                drop(entry);
                self.entries.remove(key);
                return IdempotencyCheck::New;
            }

            if entry.body_hash == body_hash {
                IdempotencyCheck::Replay(entry.response.clone())
            } else {
                IdempotencyCheck::Conflict
            }
        } else {
            IdempotencyCheck::New
        }
    }

    /// Store a response for a given idempotency key.
    pub fn store(&self, key: String, body_hash: u64, response: StoredResponse) {
        // Lazy eviction: remove some expired entries on insert
        self.evict_expired();

        // Cap total size
        if self.entries.len() >= self.max_entries {
            // Remove oldest entry by earliest created_at
            if let Some(oldest_key) = self.find_oldest_key() {
                self.entries.remove(&oldest_key);
            }
        }

        self.entries.insert(
            key,
            Entry {
                response,
                body_hash,
                created_at: Instant::now(),
            },
        );
    }

    /// Remove expired entries (up to 100 per call to bound work).
    fn evict_expired(&self) {
        let mut removed = 0;
        let expired_keys: Vec<String> = self
            .entries
            .iter()
            .filter(|e| e.created_at.elapsed() > self.ttl)
            .take(100)
            .map(|e| e.key().clone())
            .collect();

        for key in expired_keys {
            self.entries.remove(&key);
            removed += 1;
            if removed >= 100 {
                break;
            }
        }
    }

    /// Find the key of the oldest entry.
    fn find_oldest_key(&self) -> Option<String> {
        self.entries
            .iter()
            .min_by_key(|e| e.created_at)
            .map(|e| e.key().clone())
    }
}

/// Create a default idempotency store from REST config values.
///
/// # Arguments
///
/// * `ttl_seconds` - TTL for stored responses
#[must_use]
pub fn create_store(ttl_seconds: u64) -> Arc<InMemoryIdempotencyStore> {
    Arc::new(InMemoryIdempotencyStore::new(
        Duration::from_secs(ttl_seconds),
        10_000,
    ))
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
#[allow(clippy::unwrap_used)] // Reason: test code
mod tests {
    use super::*;
    use serde_json::json;

    fn make_store(ttl_secs: u64) -> InMemoryIdempotencyStore {
        InMemoryIdempotencyStore::new(Duration::from_secs(ttl_secs), 100)
    }

    fn make_response() -> StoredResponse {
        StoredResponse {
            status: 201,
            headers: vec![("x-request-id".to_string(), "abc".to_string())],
            body: Some(json!({"id": 1, "name": "Alice"})),
        }
    }

    #[test]
    fn new_key_returns_new() {
        let store = make_store(3600);
        let body_hash = InMemoryIdempotencyStore::hash_body(&json!({"name": "Alice"}));
        assert!(matches!(store.check("key1", body_hash), IdempotencyCheck::New));
    }

    #[test]
    fn stored_key_replays_response() {
        let store = make_store(3600);
        let body = json!({"name": "Alice"});
        let body_hash = InMemoryIdempotencyStore::hash_body(&body);
        let response = make_response();

        store.store("key1".to_string(), body_hash, response);

        match store.check("key1", body_hash) {
            IdempotencyCheck::Replay(stored) => {
                assert_eq!(stored.status, 201);
                assert_eq!(stored.body.as_ref().unwrap()["name"], "Alice");
            }
            other => panic!("Expected Replay, got {other:?}"),
        }
    }

    #[test]
    fn same_key_different_body_returns_conflict() {
        let store = make_store(3600);
        let body1 = json!({"name": "Alice"});
        let body2 = json!({"name": "Bob"});
        let hash1 = InMemoryIdempotencyStore::hash_body(&body1);
        let hash2 = InMemoryIdempotencyStore::hash_body(&body2);

        store.store("key1".to_string(), hash1, make_response());

        assert!(matches!(
            store.check("key1", hash2),
            IdempotencyCheck::Conflict
        ));
    }

    #[test]
    fn expired_key_treated_as_new() {
        let store = InMemoryIdempotencyStore::new(Duration::from_millis(1), 100);
        let body = json!({"name": "Alice"});
        let body_hash = InMemoryIdempotencyStore::hash_body(&body);

        store.store("key1".to_string(), body_hash, make_response());

        // Wait for TTL to expire
        std::thread::sleep(Duration::from_millis(5));

        assert!(matches!(
            store.check("key1", body_hash),
            IdempotencyCheck::New
        ));
    }

    #[test]
    fn max_entries_evicts_oldest() {
        let store = InMemoryIdempotencyStore::new(Duration::from_secs(3600), 3);
        let hash = InMemoryIdempotencyStore::hash_body(&json!({}));

        store.store("key1".to_string(), hash, make_response());
        std::thread::sleep(Duration::from_millis(1));
        store.store("key2".to_string(), hash, make_response());
        std::thread::sleep(Duration::from_millis(1));
        store.store("key3".to_string(), hash, make_response());
        std::thread::sleep(Duration::from_millis(1));

        // This should evict key1 (oldest)
        store.store("key4".to_string(), hash, make_response());

        assert!(matches!(store.check("key1", hash), IdempotencyCheck::New));
        // key2 should still be there
        assert!(matches!(store.check("key2", hash), IdempotencyCheck::Replay(_)));
    }

    #[test]
    fn body_hash_deterministic() {
        let body = json!({"name": "Alice", "age": 30});
        let hash1 = InMemoryIdempotencyStore::hash_body(&body);
        let hash2 = InMemoryIdempotencyStore::hash_body(&body);
        assert_eq!(hash1, hash2);
    }

    #[test]
    fn body_hash_different_for_different_bodies() {
        let hash1 = InMemoryIdempotencyStore::hash_body(&json!({"name": "Alice"}));
        let hash2 = InMemoryIdempotencyStore::hash_body(&json!({"name": "Bob"}));
        assert_ne!(hash1, hash2);
    }

    #[test]
    fn create_store_returns_arc() {
        let store = create_store(3600);
        let body_hash = InMemoryIdempotencyStore::hash_body(&json!({}));
        assert!(matches!(store.check("key1", body_hash), IdempotencyCheck::New));
    }
}
