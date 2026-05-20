//! Idempotency key support for REST POST mutations.
//!
//! Clients include an `Idempotency-Key` header on POST requests.  If a response
//! for that key has already been stored, it is replayed.  If the same key is
//! reused with a different request body, a 422 `IDEMPOTENCY_CONFLICT` error is
//! returned.
//!
//! GET, PUT, and DELETE are inherently idempotent — the key is ignored for those
//! methods.
//!
//! The default [`InMemoryIdempotencyStore`] uses a [`DashMap`] with TTL-based
//! expiry.  A Redis-backed implementation is available under the
//! `redis-idempotency` feature flag.

use std::{sync::Arc, time::Duration};

use tokio::time::Instant;

use dashmap::DashMap;
use serde_json::Value;
use xxhash_rust::xxh3::xxh3_64;

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// Result of checking an idempotency key.
#[derive(Debug)]
#[non_exhaustive]
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
#[cfg_attr(
    feature = "redis-idempotency",
    derive(serde::Serialize, serde::Deserialize)
)]
pub struct StoredResponse {
    /// HTTP status code.
    pub status: u16,
    /// Response headers (key, value) pairs.
    pub headers: Vec<(String, String)>,
    /// Response body (if any).
    pub body: Option<Value>,
}

/// Entry in the in-memory idempotency store.
struct Entry {
    response: StoredResponse,
    body_hash: u64,
    created_at: Instant,
}

// ---------------------------------------------------------------------------
// Trait
// ---------------------------------------------------------------------------

/// Backend-agnostic idempotency store.
///
/// Implementations must be `Send + Sync` for use in async Axum handlers.
/// Uses boxed futures for object safety (`Arc<dyn IdempotencyStore>`).
pub trait IdempotencyStore: Send + Sync {
    /// Check an idempotency key against the store.
    ///
    /// Returns [`IdempotencyCheck::New`] if no entry exists (or it has expired),
    /// [`IdempotencyCheck::Replay`] if the key matches with the same body hash,
    /// or [`IdempotencyCheck::Conflict`] if the key matches with a different body.
    fn check(
        &self,
        key: &str,
        body_hash: u64,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = IdempotencyCheck> + Send + '_>>;

    /// Store a response for a given idempotency key.
    fn store(
        &self,
        key: String,
        body_hash: u64,
        response: StoredResponse,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send + '_>>;
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

    /// Remove expired entries (up to 100 per call to bound work).
    fn evict_expired(&self) {
        let expired_keys: Vec<String> = self
            .entries
            .iter()
            .filter(|e| e.created_at.elapsed() > self.ttl)
            .take(100)
            .map(|e| e.key().clone())
            .collect();

        for key in expired_keys {
            self.entries.remove(&key);
        }
    }

    /// Find the key of the oldest entry.
    fn find_oldest_key(&self) -> Option<String> {
        self.entries.iter().min_by_key(|e| e.created_at).map(|e| e.key().clone())
    }
}

impl IdempotencyStore for InMemoryIdempotencyStore {
    fn check(
        &self,
        key: &str,
        body_hash: u64,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = IdempotencyCheck> + Send + '_>> {
        let result = if let Some(entry) = self.entries.get(key) {
            if entry.created_at.elapsed() > self.ttl {
                drop(entry);
                self.entries.remove(key);
                IdempotencyCheck::New
            } else if entry.body_hash == body_hash {
                IdempotencyCheck::Replay(entry.response.clone())
            } else {
                IdempotencyCheck::Conflict
            }
        } else {
            IdempotencyCheck::New
        };
        Box::pin(std::future::ready(result))
    }

    fn store(
        &self,
        key: String,
        body_hash: u64,
        response: StoredResponse,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send + '_>> {
        // Lazy eviction: remove some expired entries on insert
        self.evict_expired();

        // Cap total size
        if self.entries.len() >= self.max_entries {
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
        Box::pin(std::future::ready(()))
    }
}

// ---------------------------------------------------------------------------
// Redis store (feature-gated)
// ---------------------------------------------------------------------------

#[cfg(feature = "redis-idempotency")]
#[path = "redis_store.rs"]
mod redis_store;

#[cfg(feature = "redis-idempotency")]
pub use redis_store::RedisIdempotencyStore;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Hash a request body for conflict detection.
#[must_use]
pub fn hash_body(body: &Value) -> u64 {
    let bytes = serde_json::to_vec(body).unwrap_or_default();
    xxh3_64(&bytes)
}

/// Create a default idempotency store from REST config values.
///
/// # Arguments
///
/// * `ttl_seconds` - TTL for stored responses
#[must_use]
pub fn create_store(ttl_seconds: u64) -> Arc<dyn IdempotencyStore> {
    Arc::new(InMemoryIdempotencyStore::new(Duration::from_secs(ttl_seconds), 10_000))
}

/// Create an idempotency store, preferring Redis when available.
///
/// Falls back to in-memory if Redis is unavailable or the feature is disabled.
#[cfg(feature = "redis-idempotency")]
#[must_use]
pub fn create_store_with_redis(
    ttl_seconds: u64,
    redis_pool: Option<redis::aio::ConnectionManager>,
) -> Arc<dyn IdempotencyStore> {
    if let Some(pool) = redis_pool {
        Arc::new(RedisIdempotencyStore::new(pool, Duration::from_secs(ttl_seconds)))
    } else {
        create_store(ttl_seconds)
    }
}

#[cfg(test)]
mod tests;
