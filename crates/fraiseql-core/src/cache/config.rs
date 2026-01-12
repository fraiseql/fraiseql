//! Cache configuration.
//!
//! Defines configuration options for the query result cache with memory-safe bounds
//! and sensible defaults for different deployment sizes.

use serde::{Deserialize, Serialize};

/// Cache configuration with memory-safe bounds.
///
/// # Memory Safety
///
/// The cache uses a hard LRU limit to prevent unbounded growth, combined with
/// TTL-based expiry as a safety net for non-mutation changes. This ensures
/// predictable memory usage and prevents OOM conditions.
///
/// # Recommended Settings
///
/// **Small Deployments** (development, low traffic):
/// ```rust
/// use fraiseql_core::cache::CacheConfig;
///
/// let config = CacheConfig {
///     enabled: true,
///     max_entries: 1_000,
///     ttl_seconds: 3_600,  // 1 hour
///     cache_list_queries: true,
/// };
/// ```
///
/// **Medium Deployments** (10-50 QPS):
/// ```rust
/// use fraiseql_core::cache::CacheConfig;
///
/// let config = CacheConfig {
///     enabled: true,
///     max_entries: 10_000,
///     ttl_seconds: 86_400,  // 24 hours
///     cache_list_queries: true,
/// };
/// ```
///
/// **Large Deployments** (100+ QPS):
/// ```rust
/// use fraiseql_core::cache::CacheConfig;
///
/// let config = CacheConfig {
///     enabled: true,
///     max_entries: 50_000,
///     ttl_seconds: 86_400,  // 24 hours
///     cache_list_queries: true,
/// };
/// ```
///
/// **Development/Testing** (deterministic behavior):
/// ```rust
/// use fraiseql_core::cache::CacheConfig;
///
/// let config = CacheConfig {
///     enabled: false,  // Disable caching for testing
///     ..Default::default()
/// };
/// ```
///
/// # Memory Estimates
///
/// - **1,000 entries**: ~10 MB (small)
/// - **10,000 entries**: ~100 MB (medium, default)
/// - **50,000 entries**: ~500 MB (large)
///
/// Actual memory usage depends on query result sizes. These estimates assume
/// average result size of 10 KB per entry.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct CacheConfig {
    /// Enable response caching.
    ///
    /// Set to `false` for testing or debugging to ensure deterministic behavior
    /// without cached results affecting tests.
    ///
    /// Default: `true`
    pub enabled: bool,

    /// Maximum number of cached entries.
    ///
    /// When this limit is reached, the least-recently-used (LRU) entry is evicted
    /// to make room for new entries. This hard limit prevents unbounded memory growth.
    ///
    /// Recommended values:
    /// - Development: 1,000
    /// - Production (small): 10,000
    /// - Production (large): 50,000
    ///
    /// Default: 10,000 entries (~100 MB estimated memory)
    pub max_entries: usize,

    /// Time-to-live (TTL) in seconds for cached entries.
    ///
    /// Entries older than this are considered expired and will be removed on next access.
    /// This acts as a safety net for cases where invalidation might be missed (e.g.,
    /// database changes outside of mutations).
    ///
    /// Recommended values:
    /// - Development: 3,600 (1 hour)
    /// - Production: 86,400 (24 hours)
    /// - Long-lived data: 604,800 (7 days)
    ///
    /// Default: 86,400 seconds (24 hours)
    pub ttl_seconds: u64,

    /// Whether to cache list queries.
    ///
    /// List queries (e.g., `users(limit: 100)`) can have large result sets that
    /// consume significant memory. Set to `false` to only cache single-object queries.
    ///
    /// **Note**: In Phase 2, this is not yet implemented (all queries are cached).
    /// This field is reserved for future use.
    ///
    /// Default: `true`
    pub cache_list_queries: bool,
}

impl Default for CacheConfig {
    /// Default cache configuration suitable for medium-sized production deployments.
    ///
    /// - Caching enabled
    /// - 10,000 max entries (~100 MB)
    /// - 24 hour TTL
    /// - List queries cached
    fn default() -> Self {
        Self {
            enabled: true,
            max_entries: 10_000,
            ttl_seconds: 86_400,  // 24 hours
            cache_list_queries: true,
        }
    }
}

impl CacheConfig {
    /// Create cache configuration with custom max entries.
    ///
    /// Uses default values for other fields (enabled=true, 24h TTL).
    ///
    /// # Arguments
    ///
    /// * `max_entries` - Maximum number of entries in cache
    ///
    /// # Example
    ///
    /// ```rust
    /// use fraiseql_core::cache::CacheConfig;
    ///
    /// let config = CacheConfig::with_max_entries(50_000);
    /// assert_eq!(config.max_entries, 50_000);
    /// assert!(config.enabled);
    /// ```
    #[must_use]
    pub const fn with_max_entries(max_entries: usize) -> Self {
        Self {
            enabled: true,
            max_entries,
            ttl_seconds: 86_400,
            cache_list_queries: true,
        }
    }

    /// Create cache configuration with custom TTL.
    ///
    /// Uses default values for other fields (enabled=true, 10,000 entries).
    ///
    /// # Arguments
    ///
    /// * `ttl_seconds` - Time-to-live in seconds
    ///
    /// # Example
    ///
    /// ```rust
    /// use fraiseql_core::cache::CacheConfig;
    ///
    /// let config = CacheConfig::with_ttl(3_600);  // 1 hour
    /// assert_eq!(config.ttl_seconds, 3_600);
    /// assert!(config.enabled);
    /// ```
    #[must_use]
    pub const fn with_ttl(ttl_seconds: u64) -> Self {
        Self {
            enabled: true,
            max_entries: 10_000,
            ttl_seconds,
            cache_list_queries: true,
        }
    }

    /// Create cache configuration with caching disabled.
    ///
    /// Useful for testing and debugging when you want deterministic behavior
    /// without cached results.
    ///
    /// # Example
    ///
    /// ```rust
    /// use fraiseql_core::cache::CacheConfig;
    ///
    /// let config = CacheConfig::disabled();
    /// assert!(!config.enabled);
    /// ```
    #[must_use]
    pub const fn disabled() -> Self {
        Self {
            enabled: false,
            max_entries: 10_000,
            ttl_seconds: 86_400,
            cache_list_queries: true,
        }
    }

    /// Estimate memory usage in bytes for this configuration.
    ///
    /// This is a rough estimate assuming average entry size of 10 KB.
    /// Actual memory usage will vary based on query result sizes.
    ///
    /// # Returns
    ///
    /// Estimated memory usage in bytes
    ///
    /// # Example
    ///
    /// ```rust
    /// use fraiseql_core::cache::CacheConfig;
    ///
    /// let config = CacheConfig::default();
    /// let estimated_bytes = config.estimated_memory_bytes();
    /// println!("Estimated memory: {} MB", estimated_bytes / 1_000_000);
    /// ```
    #[must_use]
    pub const fn estimated_memory_bytes(&self) -> usize {
        // Rough estimate: 10 KB per entry
        const AVG_ENTRY_SIZE_BYTES: usize = 10_000;
        self.max_entries * AVG_ENTRY_SIZE_BYTES
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = CacheConfig::default();
        assert!(config.enabled);
        assert_eq!(config.max_entries, 10_000);
        assert_eq!(config.ttl_seconds, 86_400);
        assert!(config.cache_list_queries);
    }

    #[test]
    fn test_with_max_entries() {
        let config = CacheConfig::with_max_entries(50_000);
        assert_eq!(config.max_entries, 50_000);
        assert!(config.enabled);
        assert_eq!(config.ttl_seconds, 86_400);
    }

    #[test]
    fn test_with_ttl() {
        let config = CacheConfig::with_ttl(3_600);
        assert_eq!(config.ttl_seconds, 3_600);
        assert!(config.enabled);
        assert_eq!(config.max_entries, 10_000);
    }

    #[test]
    fn test_disabled() {
        let config = CacheConfig::disabled();
        assert!(!config.enabled);
    }

    #[test]
    fn test_estimated_memory() {
        let config = CacheConfig::with_max_entries(10_000);
        let estimated = config.estimated_memory_bytes();
        // Should be roughly 100 MB (10,000 * 10 KB)
        assert_eq!(estimated, 100_000_000);
    }

    #[test]
    fn test_serialization() {
        let config = CacheConfig::default();
        let json = serde_json::to_string(&config).unwrap();
        let deserialized: CacheConfig = serde_json::from_str(&json).unwrap();

        assert_eq!(config.enabled, deserialized.enabled);
        assert_eq!(config.max_entries, deserialized.max_entries);
        assert_eq!(config.ttl_seconds, deserialized.ttl_seconds);
    }
}
