//! Action result caching system for improved performance.
//!
//! This module provides trait-based caching of action results with configurable TTL,
//! enabling significant performance improvements for repeated actions.
//! Redis implementation available with `caching` feature.
//!
//! # Problem Solved
//!
//! Without caching:
//! - Every event executes all actions
//! - Repeated events call external APIs redundantly
//! - Unnecessary API latency (100ms+ per action)
//! - API rate limits hit sooner
//!
//! With caching:
//! - First execution: run action, cache result (1 second TTL by default)
//! - Repeated events within TTL: return cached result instantly
//! - After TTL expiry: fetch fresh result
//!
//! # Performance Impact
//!
//! Example with 3 actions on an order event:
//! - Without cache: 100ms + 100ms + 100ms = 300ms
//! - With cache (hit): 1ms + 1ms + 1ms = 3ms
//! - **100x faster** for cache hits
//!
//! # Architecture
//!
//! ```text
//! Action execution request
//!     ↓
//! Check cache: Redis GET "cache:{action}:{event_hash}"
//!     ↓
//! If hit (exists) → Return cached result
//! If miss (not found) → Execute action
//!     ↓ (on execution)
//! Cache result: Redis SETEX with TTL
//!     ↓
//! Return result
//! ```
//!
//! # Cache Key Format
//!
//! Keys use semantic naming:
//! - Format: `cache:v1:{action_type}:{event_hash}:{entity_type}:{entity_id}`
//! - Example: `cache:v1:email_action:sha256hash:Order:order-123`
//! - Hash prevents sensitive data in keys
//! - Entity info in key for visibility/debugging

#[cfg(feature = "caching")]
pub mod redis;

use serde::{Deserialize, Serialize};

use crate::error::Result;

/// Cache backend abstraction.
///
/// Provides persistent caching of action results with TTL support.
/// Implementations handle the actual storage mechanism (Redis, Memcached, etc.).
///
/// # Trait Objects
///
/// This trait is object-safe and can be used as `Arc<dyn CacheBackend>`.
#[async_trait::async_trait]
pub trait CacheBackend: Send + Sync + Clone {
    /// Get a cached action result.
    ///
    /// Returns `Ok(Some(result))` if cached and not expired.
    /// Returns `Ok(None)` if not cached or expired.
    ///
    /// # Arguments
    ///
    /// * `cache_key` - The cache key for this action result
    ///
    /// # Errors
    ///
    /// Returns error if cache operation fails
    async fn get(&self, cache_key: &str) -> Result<Option<CachedActionResult>>;

    /// Store an action result in cache with TTL.
    ///
    /// # Arguments
    ///
    /// * `cache_key` - The cache key for this action result
    /// * `result` - The action result to cache
    ///
    /// # Errors
    ///
    /// Returns error if cache operation fails
    async fn set(&self, cache_key: &str, result: &CachedActionResult) -> Result<()>;

    /// Get the default cache TTL in seconds.
    fn ttl_seconds(&self) -> u64;

    /// Set the default cache TTL in seconds.
    fn set_ttl_seconds(&mut self, seconds: u64);

    /// Remove a cached result (for invalidation).
    ///
    /// # Arguments
    ///
    /// * `cache_key` - The cache key to invalidate
    ///
    /// # Errors
    ///
    /// Returns error if cache operation fails
    async fn invalidate(&self, cache_key: &str) -> Result<()>;

    /// Clear all cached results (for testing/reset).
    ///
    /// # Errors
    ///
    /// Returns error if cache operation fails
    async fn clear_all(&self) -> Result<()>;
}

/// Object-safe cache backend trait for trait objects.
///
/// This is a subset of `CacheBackend` designed to be object-safe (works as `dyn CacheBackendDyn`).
/// Unlike `CacheBackend`, it does not require `Clone`, making it suitable for use as a trait object.
#[async_trait::async_trait]
pub trait CacheBackendDyn: Send + Sync {
    /// Get a cached action result.
    ///
    /// Returns `Ok(Some(result))` if cached and not expired.
    /// Returns `Ok(None)` if not cached or expired.
    ///
    /// # Arguments
    ///
    /// * `cache_key` - The cache key for this action result
    ///
    /// # Errors
    ///
    /// Returns error if cache operation fails
    async fn get(&self, cache_key: &str) -> Result<Option<CachedActionResult>>;

    /// Store an action result in cache with TTL.
    ///
    /// # Arguments
    ///
    /// * `cache_key` - The cache key for this action result
    /// * `result` - The action result to cache
    ///
    /// # Errors
    ///
    /// Returns error if cache operation fails
    async fn set(&self, cache_key: &str, result: &CachedActionResult) -> Result<()>;

    /// Get the default cache TTL in seconds.
    fn ttl_seconds(&self) -> u64;
}

/// Cached action result with metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CachedActionResult {
    /// Type of action that was executed
    pub action_type:    String,
    /// Whether the action succeeded
    pub success:        bool,
    /// Status message
    pub message:        String,
    /// Execution time in milliseconds
    pub duration_ms:    f64,
    /// When this result was cached (Unix timestamp)
    pub cached_at_unix: i64,
}

impl CachedActionResult {
    /// Create a new cached action result.
    #[must_use]
    pub fn new(action_type: String, success: bool, message: String, duration_ms: f64) -> Self {
        Self {
            action_type,
            success,
            message,
            duration_ms,
            cached_at_unix: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs() as i64,
        }
    }

    /// Check if this cached result is "fresh" (low latency).
    ///
    /// Results with <10ms duration are considered fresh (likely cached).
    #[must_use]
    pub fn is_fresh(&self) -> bool {
        self.duration_ms < 10.0
    }
}

/// Cache statistics for monitoring.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheStats {
    /// Total cache requests
    pub total_requests:      u64,
    /// Cache hits (result found and returned)
    pub cache_hits:          u64,
    /// Cache misses (result not found, required execution)
    pub cache_misses:        u64,
    /// Cache hit rate (0.0 - 1.0)
    pub hit_rate:            f64,
    /// Average latency for cache hits (ms)
    pub avg_hit_latency_ms:  f64,
    /// Average latency for cache misses (ms)
    pub avg_miss_latency_ms: f64,
}

impl CacheStats {
    /// Create new cache statistics.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            total_requests:      0,
            cache_hits:          0,
            cache_misses:        0,
            hit_rate:            0.0,
            avg_hit_latency_ms:  0.0,
            avg_miss_latency_ms: 0.0,
        }
    }

    /// Record a cache request.
    ///
    /// # Arguments
    ///
    /// * `is_hit` - Whether this was a cache hit
    /// * `latency_ms` - Request latency in milliseconds
    pub fn record(&mut self, is_hit: bool, latency_ms: f64) {
        self.total_requests += 1;

        if is_hit {
            self.cache_hits += 1;
            self.avg_hit_latency_ms =
                self.avg_hit_latency_ms.mul_add(self.cache_hits as f64 - 1.0, latency_ms)
                    / self.cache_hits as f64;
        } else {
            self.cache_misses += 1;
            self.avg_miss_latency_ms =
                self.avg_miss_latency_ms.mul_add(self.cache_misses as f64 - 1.0, latency_ms)
                    / self.cache_misses as f64;
        }

        if self.total_requests > 0 {
            self.hit_rate = self.cache_hits as f64 / self.total_requests as f64;
        }
    }

    /// Reset statistics.
    pub fn reset(&mut self) {
        self.total_requests = 0;
        self.cache_hits = 0;
        self.cache_misses = 0;
        self.hit_rate = 0.0;
        self.avg_hit_latency_ms = 0.0;
        self.avg_miss_latency_ms = 0.0;
    }
}

impl Default for CacheStats {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cached_action_result_new() {
        let result =
            CachedActionResult::new("email".to_string(), true, "Email sent".to_string(), 125.5);

        assert_eq!(result.action_type, "email");
        assert!(result.success);
        assert!((result.duration_ms - 125.5).abs() < f64::EPSILON);
        assert!(result.cached_at_unix > 0);
    }

    #[test]
    fn test_cached_action_result_is_fresh() {
        let fresh =
            CachedActionResult::new("cache".to_string(), true, "From cache".to_string(), 5.0);
        assert!(fresh.is_fresh());

        let not_fresh =
            CachedActionResult::new("api".to_string(), true, "From API".to_string(), 100.0);
        assert!(!not_fresh.is_fresh());
    }

    #[test]
    fn test_cache_stats_new() {
        let stats = CacheStats::new();
        assert_eq!(stats.total_requests, 0);
        assert_eq!(stats.cache_hits, 0);
        assert_eq!(stats.cache_misses, 0);
        assert_eq!(stats.hit_rate, 0.0);
    }

    #[test]
    fn test_cache_stats_record_hit() {
        let mut stats = CacheStats::new();
        stats.record(true, 2.0);

        assert_eq!(stats.total_requests, 1);
        assert_eq!(stats.cache_hits, 1);
        assert_eq!(stats.cache_misses, 0);
        assert_eq!(stats.hit_rate, 1.0);
        assert!((stats.avg_hit_latency_ms - 2.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_cache_stats_record_miss() {
        let mut stats = CacheStats::new();
        stats.record(false, 150.0);

        assert_eq!(stats.total_requests, 1);
        assert_eq!(stats.cache_hits, 0);
        assert_eq!(stats.cache_misses, 1);
        assert_eq!(stats.hit_rate, 0.0);
        assert!((stats.avg_miss_latency_ms - 150.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_cache_stats_hit_rate_calculation() {
        let mut stats = CacheStats::new();
        // 8 hits at ~2ms each
        for _ in 0..8 {
            stats.record(true, 2.0);
        }
        // 2 misses at ~150ms each
        for _ in 0..2 {
            stats.record(false, 150.0);
        }

        assert_eq!(stats.total_requests, 10);
        assert_eq!(stats.cache_hits, 8);
        assert_eq!(stats.cache_misses, 2);
        assert!((stats.hit_rate - 0.8).abs() < f64::EPSILON);
        assert!((stats.avg_hit_latency_ms - 2.0).abs() < f64::EPSILON);
        assert!((stats.avg_miss_latency_ms - 150.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_cache_stats_reset() {
        let mut stats = CacheStats::new();
        stats.record(true, 2.0);
        stats.record(false, 150.0);

        stats.reset();

        assert_eq!(stats.total_requests, 0);
        assert_eq!(stats.cache_hits, 0);
        assert_eq!(stats.cache_misses, 0);
        assert_eq!(stats.hit_rate, 0.0);
        assert_eq!(stats.avg_hit_latency_ms, 0.0);
        assert_eq!(stats.avg_miss_latency_ms, 0.0);
    }
}
