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

use dashmap::DashMap;
use serde_json::Value;
use tokio::time::Instant;
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
    pub status:  u16,
    /// Response headers (key, value) pairs.
    pub headers: Vec<(String, String)>,
    /// Response body (if any).
    pub body:    Option<Value>,
}

/// Entry in the in-memory idempotency store.
struct Entry {
    response:   StoredResponse,
    body_hash:  u64,
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
        key: &ScopedIdempotencyKey,
        body_hash: u64,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = IdempotencyCheck> + Send + '_>>;

    /// Store a response for a given idempotency key.
    fn store(
        &self,
        key: ScopedIdempotencyKey,
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
    entries:     DashMap<String, Entry>,
    ttl:         Duration,
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
        key: &ScopedIdempotencyKey,
        body_hash: u64,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = IdempotencyCheck> + Send + '_>> {
        let key = key.as_str();
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
        key: ScopedIdempotencyKey,
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
            key.as_str().to_string(),
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
mod redis_store;

#[cfg(feature = "redis-idempotency")]
pub use redis_store::RedisIdempotencyStore;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Hash a request body for conflict detection.
///
/// The body is **normalized** (object keys sorted at every level) before serialization.
/// Since `serde_json/preserve_order` became an unconditional workspace feature, `Value`
/// preserves insertion order in every build, so `{"a":1,"b":2}` and `{"b":2,"a":1}` —
/// the same request — rendered to different bytes and hashed differently. This hash is
/// the conflict detector for the whole `Idempotency-Key` contract, so a retry whose
/// client had re-serialized the body (Go's `encoding/json` sorts map keys; Python retry
/// wrappers commonly use `sort_keys=True`) received `409 Conflict` instead of the cached
/// response — defeating the feature for exactly the clients that need it (`#911`).
///
/// Reuses `fraiseql_core`'s APQ normalizer rather than adding a second recursive sorter.
#[must_use]
pub fn hash_body(body: &Value) -> u64 {
    let normalized = fraiseql_core::apq::normalize_json_value(body.clone());
    // `to_vec` on a `Value` is infallible; `unwrap_or_default` would have hashed every
    // unserializable body to the empty vector, making an all-bodies-collide path look
    // acceptable. Serialize to a String instead, which cannot fail for a `Value`.
    let bytes = serde_json::to_string(&normalized).unwrap_or_else(|_| normalized.to_string());
    xxh3_64(bytes.as_bytes())
}

/// The scope an idempotency key is valid within.
///
/// `Idempotency-Key` is a client-chosen opaque string. Used verbatim as the store key —
/// which is what the REST handler did — it collides across everything that shares a
/// process: the same key and body on `POST /users` and `POST /orders` replayed each
/// other's stored response, and two tenants retrying an identical request under a natural
/// key such as `order-42` received each other's results (`#915`).
///
/// The scope is a parameter of the store operations rather than something the caller
/// pre-hashes, so an implementation cannot silently omit it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IdempotencyScope {
    /// Tenant the request belongs to, when the deployment resolves one.
    pub tenant: Option<String>,
    /// HTTP method, so a key cannot cross verbs.
    pub method: String,
    /// Resolved resource path, so a key cannot cross resources.
    pub path:   String,
}

impl IdempotencyScope {
    /// Compose the storage key for a client-supplied `Idempotency-Key`.
    ///
    /// Segments are length-prefixed so no combination of tenant, method, path and key can
    /// be forged into another by embedding the separator.
    #[must_use]
    pub fn key(&self, client_key: &str) -> ScopedIdempotencyKey {
        let tenant = self.tenant.as_deref().unwrap_or("");
        let mut out = String::with_capacity(
            tenant.len() + self.method.len() + self.path.len() + client_key.len() + 16,
        );
        for segment in [tenant, self.method.as_str(), self.path.as_str(), client_key] {
            out.push_str(&segment.len().to_string());
            out.push(':');
            out.push_str(segment);
            out.push('|');
        }
        ScopedIdempotencyKey(out)
    }
}

/// A storage key that has been through [`IdempotencyScope`].
///
/// The store API takes this rather than a `&str` on purpose: it is the only constructor,
/// so an implementation or a call site *cannot* accidentally key on the raw client-
/// supplied header value. That is what `#915` was — the omission was invisible because
/// the signature accepted the unscoped string just as happily.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScopedIdempotencyKey(String);

impl ScopedIdempotencyKey {
    /// The composed key, for use as a map or Redis key.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// TTL for the GraphQL mutation path's idempotency store (#747): 24 hours.
///
/// Sized to the saga crash-recovery window, not the transport-retry window: a
/// coordinator's in-flight retries land within seconds, but a crash-left saga is
/// only re-driven once it passes the recovery staleness threshold, and its
/// re-dispatch of an in-doubt step can arrive hours after the original attempt.
/// The stored entry must still be there for that replay to deduplicate.
pub const GRAPHQL_IDEMPOTENCY_TTL_SECS: u64 = 86_400;

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
