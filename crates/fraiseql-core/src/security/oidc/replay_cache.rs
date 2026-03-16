//! JWT replay prevention via a jti (JWT ID) replay cache.
//!
//! Each validated JWT token carries a unique `jti` claim. This module stores
//! seen `jti` values and rejects any token whose `jti` has been seen before,
//! preventing stolen-token replay attacks within the token's remaining validity
//! window.
//!
//! # Backends
//!
//! - **Redis** (`jwt-replay` feature): distributed, survives server restarts.
//! - **Memory** (always available): single-process, resets on restart; suitable
//!   for testing or single-instance deployments.
//!
//! # Failure policy
//!
//! When Redis is unavailable, behavior is controlled by [`FailurePolicy`]:
//! - [`FailurePolicy::FailOpen`] (default): accept the token and log a warning.
//!   Prevents auth outages during Redis downtime at the cost of reduced replay
//!   protection.
//! - [`FailurePolicy::FailClosed`]: reject the token. Maximum security, but any
//!   Redis hiccup will cause auth failures.

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use async_trait::async_trait;
use tracing::warn;

// ============================================================================
// Error type
// ============================================================================

/// Error returned by [`ReplayCacheBackend::check_and_record`].
#[derive(Debug, thiserror::Error)]
pub enum ReplayCacheError {
    /// The `jti` was already seen — this is a replayed token.
    #[error("JWT token has already been used (jti replay detected)")]
    Replayed,
    /// The backend returned an unexpected error.
    #[error("Replay cache backend error: {0}")]
    Backend(String),
}

// ============================================================================
// Failure policy
// ============================================================================

/// Policy controlling what happens when the replay-cache backend is unavailable.
#[derive(Debug, Clone, Copy, Default)]
pub enum FailurePolicy {
    /// Accept the token and log a warning. Prevents auth outages during backend
    /// downtime at the cost of reduced replay protection during the outage.
    #[default]
    FailOpen,
    /// Reject the token. Maximum security, but any backend hiccup causes auth
    /// failures.
    FailClosed,
}

// ============================================================================
// Backend trait
// ============================================================================

/// A backend that stores and checks seen JWT IDs.
#[async_trait]
pub trait ReplayCacheBackend: Send + Sync {
    /// Check whether `jti` has been seen before, and record it if not.
    ///
    /// # Returns
    ///
    /// - `Ok(())` if this is the first time the `jti` has been seen (accepted).
    /// - `Err(ReplayCacheError::Replayed)` if the `jti` was already stored.
    /// - `Err(ReplayCacheError::Backend(_))` on a transient backend error.
    ///
    /// # Errors
    ///
    /// Returns [`ReplayCacheError::Replayed`] when replay is detected.
    /// Returns [`ReplayCacheError::Backend`] on storage failure.
    async fn check_and_record(
        &self,
        jti: &str,
        ttl: Duration,
    ) -> Result<(), ReplayCacheError>;
}

// ============================================================================
// ReplayCache — the public façade
// ============================================================================

/// Global counter for JWT replay rejections.
static JWT_REPLAY_REJECTED_TOTAL: AtomicU64 = AtomicU64::new(0);
/// Global counter for replay-cache backend errors (Redis failures, etc.).
static JWT_REPLAY_CACHE_ERRORS_TOTAL: AtomicU64 = AtomicU64::new(0);

/// Return the cumulative count of JWT replay rejections since process start.
#[must_use]
pub fn jwt_replay_rejected_total() -> u64 {
    JWT_REPLAY_REJECTED_TOTAL.load(Ordering::Relaxed)
}

/// Return the cumulative count of replay-cache backend errors since process start.
#[must_use]
pub fn jwt_replay_cache_errors_total() -> u64 {
    JWT_REPLAY_CACHE_ERRORS_TOTAL.load(Ordering::Relaxed)
}

/// JWT replay prevention cache.
///
/// Wraps a [`ReplayCacheBackend`] with a configurable [`FailurePolicy`] and
/// Prometheus-compatible counters.
pub struct ReplayCache {
    backend: Box<dyn ReplayCacheBackend>,
    policy:  FailurePolicy,
}

impl ReplayCache {
    /// Create a new `ReplayCache` wrapping the given backend.
    #[must_use]
    pub fn new(backend: impl ReplayCacheBackend + 'static) -> Self {
        Self {
            backend: Box::new(backend),
            policy:  FailurePolicy::FailOpen,
        }
    }

    /// Set the failure policy for backend errors.
    #[must_use]
    pub const fn with_policy(mut self, policy: FailurePolicy) -> Self {
        self.policy = policy;
        self
    }

    /// Check and record the given `jti` with the given TTL.
    ///
    /// # Errors
    ///
    /// Returns `Err(ReplayCacheError::Replayed)` when replay is detected.
    /// Backend errors are handled according to the configured [`FailurePolicy`].
    pub async fn check_and_record(
        &self,
        jti: &str,
        ttl: Duration,
    ) -> Result<(), ReplayCacheError> {
        match self.backend.check_and_record(jti, ttl).await {
            Ok(()) => Ok(()),
            Err(ReplayCacheError::Replayed) => {
                JWT_REPLAY_REJECTED_TOTAL.fetch_add(1, Ordering::Relaxed);
                Err(ReplayCacheError::Replayed)
            }
            Err(ReplayCacheError::Backend(msg)) => {
                JWT_REPLAY_CACHE_ERRORS_TOTAL.fetch_add(1, Ordering::Relaxed);
                match self.policy {
                    FailurePolicy::FailOpen => {
                        warn!(
                            error = %msg,
                            "JWT replay cache backend error — failing open (token accepted). \
                             Replay protection is degraded while the backend is unavailable."
                        );
                        Ok(())
                    }
                    FailurePolicy::FailClosed => {
                        Err(ReplayCacheError::Backend(msg))
                    }
                }
            }
        }
    }
}

// ============================================================================
// In-memory backend (always compiled in; useful for tests + single-process)
// ============================================================================

/// In-memory JWT replay cache backend.
///
/// Uses a `DashMap` for lock-free concurrent access. TTL is enforced by storing
/// the expiry timestamp alongside each entry and lazily evicting on lookup.
///
/// **Not distributed**: each process has its own cache. Use the Redis backend
/// for multi-instance deployments.
pub struct MemoryReplayCache {
    store: dashmap::DashMap<String, std::time::Instant>,
}

impl MemoryReplayCache {
    /// Create a new in-memory replay cache.
    #[must_use]
    pub fn new() -> Self {
        Self {
            store: dashmap::DashMap::new(),
        }
    }
}

impl Default for MemoryReplayCache {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl ReplayCacheBackend for MemoryReplayCache {
    async fn check_and_record(
        &self,
        jti: &str,
        ttl: Duration,
    ) -> Result<(), ReplayCacheError> {
        let now = std::time::Instant::now();
        let expiry = now + ttl;

        // Remove expired entry if present (lazy eviction).
        if let Some(existing) = self.store.get(jti) {
            if *existing > now {
                // Still valid — this is a replay.
                return Err(ReplayCacheError::Replayed);
            }
            drop(existing);
        }

        // Insert (or re-insert after expiry).
        self.store.insert(jti.to_string(), expiry);
        Ok(())
    }
}

// ============================================================================
// Redis backend (compiled in with `jwt-replay` feature)
// ============================================================================

/// Redis-backed JWT replay cache backend.
///
/// Uses `SET key 1 EX {ttl_secs} NX` (SET if Not eXists) to atomically record
/// a `jti` and detect replays: if the key was not set because it already existed,
/// the token is a replay.
#[cfg(feature = "jwt-replay")]
pub struct RedisReplayCache {
    pool:       redis::aio::ConnectionManager,
    key_prefix: String,
}

#[cfg(feature = "jwt-replay")]
impl RedisReplayCache {
    /// Connect to Redis and create the replay cache.
    ///
    /// # Errors
    ///
    /// Returns an error if the Redis URL is invalid or the connection fails.
    pub async fn new(redis_url: &str) -> Result<Self, ReplayCacheError> {
        Self::with_prefix(redis_url, "fraiseql:jti:").await
    }

    /// Connect to Redis with a custom key prefix (useful for multi-tenant isolation).
    ///
    /// # Errors
    ///
    /// Returns an error if the Redis URL is invalid or the connection fails.
    pub async fn with_prefix(
        redis_url: &str,
        key_prefix: &str,
    ) -> Result<Self, ReplayCacheError> {
        let client = redis::Client::open(redis_url)
            .map_err(|e| ReplayCacheError::Backend(format!("invalid Redis URL: {e}")))?;
        let pool = client
            .get_connection_manager()
            .await
            .map_err(|e| ReplayCacheError::Backend(format!("Redis connection failed: {e}")))?;
        Ok(Self {
            pool,
            key_prefix: key_prefix.to_string(),
        })
    }

    fn key(&self, jti: &str) -> String {
        format!("{}{}", self.key_prefix, jti)
    }
}

#[cfg(feature = "jwt-replay")]
#[async_trait]
impl ReplayCacheBackend for RedisReplayCache {
    async fn check_and_record(
        &self,
        jti: &str,
        ttl: Duration,
    ) -> Result<(), ReplayCacheError> {
        use redis::AsyncCommands;

        let key = self.key(jti);
        let ttl_secs = ttl.as_secs().max(1);
        let mut conn = self.pool.clone();

        // SET key 1 EX ttl_secs NX
        // Returns true if the key was set (first use), false if it already existed (replay).
        let was_set: bool = conn
            .set_options(
                &key,
                1u8,
                redis::SetOptions::default()
                    .conditional_set(redis::ExistenceCheck::NX)
                    .with_expiration(redis::SetExpiry::EX(ttl_secs)),
            )
            .await
            .map_err(|e| ReplayCacheError::Backend(format!("Redis SET NX failed: {e}")))?;

        if was_set {
            Ok(())
        } else {
            Err(ReplayCacheError::Replayed)
        }
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

    use super::*;

    #[tokio::test]
    async fn test_first_use_accepted() {
        let cache = ReplayCache::new(MemoryReplayCache::new());
        let result = cache
            .check_and_record("jti-abc", Duration::from_secs(900))
            .await;
        assert!(result.is_ok(), "first use should be accepted");
    }

    #[tokio::test]
    async fn test_replay_rejected() {
        let cache = ReplayCache::new(MemoryReplayCache::new());
        cache
            .check_and_record("jti-abc", Duration::from_secs(900))
            .await
            .unwrap();
        let result = cache
            .check_and_record("jti-abc", Duration::from_secs(900))
            .await;
        assert!(
            matches!(result, Err(ReplayCacheError::Replayed)),
            "second use of same jti should be rejected"
        );
    }

    #[tokio::test]
    async fn test_different_jtis_accepted_independently() {
        let cache = ReplayCache::new(MemoryReplayCache::new());
        cache
            .check_and_record("jti-1", Duration::from_secs(900))
            .await
            .unwrap();
        let result = cache
            .check_and_record("jti-2", Duration::from_secs(900))
            .await;
        assert!(result.is_ok(), "different jti should be accepted");
    }

    #[tokio::test]
    async fn test_fail_open_policy_on_backend_error() {
        struct AlwaysErrorBackend;

        #[async_trait]
        impl ReplayCacheBackend for AlwaysErrorBackend {
            async fn check_and_record(
                &self,
                _jti: &str,
                _ttl: Duration,
            ) -> Result<(), ReplayCacheError> {
                Err(ReplayCacheError::Backend("simulated error".to_string()))
            }
        }

        let cache =
            ReplayCache::new(AlwaysErrorBackend).with_policy(FailurePolicy::FailOpen);
        let result = cache
            .check_and_record("jti-xyz", Duration::from_secs(900))
            .await;
        assert!(result.is_ok(), "fail-open should accept on backend error");
    }

    #[tokio::test]
    async fn test_fail_closed_policy_on_backend_error() {
        struct AlwaysErrorBackend;

        #[async_trait]
        impl ReplayCacheBackend for AlwaysErrorBackend {
            async fn check_and_record(
                &self,
                _jti: &str,
                _ttl: Duration,
            ) -> Result<(), ReplayCacheError> {
                Err(ReplayCacheError::Backend("simulated error".to_string()))
            }
        }

        let cache =
            ReplayCache::new(AlwaysErrorBackend).with_policy(FailurePolicy::FailClosed);
        let result = cache
            .check_and_record("jti-xyz", Duration::from_secs(900))
            .await;
        assert!(result.is_err(), "fail-closed should reject on backend error");
    }

    #[tokio::test]
    async fn test_replay_counter_increments() {
        let before = jwt_replay_rejected_total();
        let cache = ReplayCache::new(MemoryReplayCache::new());
        cache
            .check_and_record("jti-counter", Duration::from_secs(900))
            .await
            .unwrap();
        let _ = cache
            .check_and_record("jti-counter", Duration::from_secs(900))
            .await;
        let after = jwt_replay_rejected_total();
        assert_eq!(after, before + 1, "replay counter should have incremented");
    }
}
