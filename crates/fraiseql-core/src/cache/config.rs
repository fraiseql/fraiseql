//! Cache configuration.
//!
//! Defines configuration options for the query result cache.
//!
//! # Important: Caching is Disabled by Default (v2.0.0-rc.12+)
//!
//! **FraiseQL v2.0.0-rc.12 changed the default: caching is now DISABLED.**
//!
//! FraiseQL uses precomputed views (tv_* tables) and optimized PostgreSQL queries
//! that are typically faster than cache overhead for most use cases. Caching only
//! provides benefit for specific scenarios (see below).
//!
//! # When to Enable Caching
//!
//! Enable caching (`enabled = true`) **only** if you have:
//!
//! 1. **Federation with slow external services** (>100ms response times)
//! 2. **Expensive computations** not covered by precomputed views
//! 3. **Very high-frequency repeated queries** (>1000 QPS with identical parameters)
//!
//! # When NOT to Enable Caching
//!
//! Don't enable caching for:
//! - Simple lookups (faster to query PostgreSQL directly)
//! - Standard CRUD operations on precomputed views
//! - Low-traffic applications (<100 QPS)
//! - Any workload where Issue #40 analysis applies
//!
//! # Configuration Examples
//!
//! **Default (recommended for most deployments):**
//! ```rust
//! use fraiseql_core::cache::CacheConfig;
//!
//! let config = CacheConfig::default(); // enabled = false
//! ```
//!
//! **Federation with external services:**
//! ```rust
//! use fraiseql_core::cache::CacheConfig;
//!
//! let config = CacheConfig::enabled();
//! ```
//!
//! **Custom cache size (if enabled):**
//! ```rust
//! use fraiseql_core::cache::CacheConfig;
//!
//! let config = CacheConfig {
//!     enabled: true,
//!     max_entries: 5_000,
//!     ttl_seconds: 3_600, // 1 hour
//!     cache_list_queries: true,
//!     ..Default::default()
//! };
//! ```
//!
//! # Memory Estimates (if enabled)
//!
//! - **1,000 entries**: ~10 MB
//! - **10,000 entries**: ~100 MB
//! - **50,000 entries**: ~500 MB
//!
//! Actual memory usage depends on query result sizes.

use serde::{Deserialize, Serialize};

/// Controls what happens when caching is enabled in a multi-tenant deployment but
/// Row-Level Security does not appear to be active.
///
/// Configure via `rls_enforcement` in `CacheConfig` or `fraiseql.toml`.
///
/// # Security implication
///
/// Without RLS, all authenticated users sharing the same query and variables will
/// receive the **same cached response**, potentially leaking data across tenants.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum RlsEnforcement {
    /// Refuse server startup if RLS appears inactive (default, safest).
    ///
    /// Use this in production to prevent silent cross-tenant data leakage.
    #[default]
    Error,

    /// Log a warning and continue if RLS appears inactive.
    ///
    /// Use during migration or for non-critical workloads.
    Warn,

    /// Skip the RLS check entirely.
    ///
    /// Use for single-tenant deployments where RLS is not needed.
    Off,
}

/// Cache configuration - **disabled by default** as of v2.0.0-rc.12.
///
/// FraiseQL's architecture (precomputed views + optimized PostgreSQL) makes
/// caching unnecessary for most use cases. Enable only for federation or
/// expensive computations.
///
/// # Key Changes in rc.12
///
/// - `enabled` now defaults to `false` (was `true`)
/// - `with_max_entries()` and `with_ttl()` also set `enabled: false`
/// - New `enabled()` constructor for explicit opt-in
///
/// See module documentation for detailed guidance.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct CacheConfig {
    /// Enable response caching.
    ///
    /// **Default: `false`** (changed from `true` in v2.0.0-rc.12)
    ///
    /// Enable only for:
    /// - Federation with slow external services
    /// - Expensive computations not covered by precomputed views
    /// - High-frequency repeated queries with identical parameters
    ///
    /// See Issue #40 for performance analysis.
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
    /// consume significant memory. Set to `false` to only cache single-object queries
    /// (results with a single row). Results with more than one row are skipped.
    ///
    /// Default: `true`
    pub cache_list_queries: bool,

    /// Row-Level Security enforcement mode for multi-tenant deployments.
    ///
    /// When caching is enabled alongside a multi-tenant schema (detected via
    /// `is_multi_tenant()` on the compiled schema), FraiseQL checks that RLS is active.
    /// Without RLS, two users sharing the same query may receive each other's data
    /// from the cache.
    ///
    /// | Mode | Behaviour |
    /// |------|-----------|
    /// | `Error` | Server refuses to start (default, safest) |
    /// | `Warn` | Logs a warning and continues |
    /// | `Off` | Skips the check (single-tenant deployments) |
    ///
    /// Default: [`RlsEnforcement::Error`]
    #[serde(default)]
    pub rls_enforcement: RlsEnforcement,

    /// Maximum bytes for a single cache entry. Entries exceeding this are silently skipped.
    ///
    /// Prevents a single oversized response from consuming a disproportionate share of
    /// the cache. The size is estimated by serializing the result to JSON and measuring
    /// the byte length.
    ///
    /// Default: `None` (no per-entry limit). Suggested value: 10 MB (10_485_760).
    #[serde(default)]
    pub max_entry_bytes: Option<usize>,

    /// Maximum total bytes across all cache entries. Triggers LRU eviction when exceeded.
    ///
    /// When set, `put()` checks whether adding the new entry would exceed the budget.
    /// If the budget is already exceeded the entry is silently skipped (the LRU count
    /// limit continues to apply independently).
    ///
    /// Default: `None` (no total limit). Suggested value: 1 GB (1_073_741_824).
    #[serde(default)]
    pub max_total_bytes: Option<usize>,
}

impl Default for CacheConfig {
    /// Default cache configuration - **DISABLED by default** as of v2.0.0-rc.12.
    ///
    /// FraiseQL uses precomputed views (tv_* tables) and optimized PostgreSQL queries
    /// that are typically faster than cache overhead for most use cases.
    ///
    /// Enable caching ONLY if you have:
    /// - Federation with slow external services
    /// - Expensive computations not covered by precomputed views
    /// - High-frequency repeated queries (>1000 QPS with same params)
    ///
    /// See Issue #40 for performance analysis.
    ///
    /// # Current Default
    /// - **Caching: DISABLED**
    /// - 10,000 max entries (~100 MB if enabled)
    /// - 24 hour TTL
    /// - List queries cached (when enabled)
    fn default() -> Self {
        Self {
            enabled:            false, // CHANGED in rc.12: Disabled by default
            max_entries:        10_000,
            ttl_seconds:        86_400, // 24 hours
            cache_list_queries: true,
            rls_enforcement:    RlsEnforcement::Error,
            max_entry_bytes:    None,
            max_total_bytes:    None,
        }
    }
}

impl CacheConfig {
    /// Create cache configuration with custom max entries.
    ///
    /// Uses default values for other fields (**enabled=false**, 24h TTL).
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
    /// assert!(!config.enabled); // Disabled by default
    /// ```
    #[must_use]
    pub const fn with_max_entries(max_entries: usize) -> Self {
        Self {
            enabled: false, // Consistent with new default
            max_entries,
            ttl_seconds: 86_400,
            cache_list_queries: true,
            rls_enforcement: RlsEnforcement::Error,
            max_entry_bytes: None,
            max_total_bytes: None,
        }
    }

    /// Create cache configuration with custom TTL.
    ///
    /// Uses default values for other fields (**enabled=false**, 10,000 entries).
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
    /// assert!(!config.enabled); // Disabled by default
    /// ```
    #[must_use]
    pub const fn with_ttl(ttl_seconds: u64) -> Self {
        Self {
            enabled: false, // Consistent with new default
            max_entries: 10_000,
            ttl_seconds,
            cache_list_queries: true,
            rls_enforcement: RlsEnforcement::Error,
            max_entry_bytes: None,
            max_total_bytes: None,
        }
    }

    /// Create cache configuration with caching **enabled**.
    ///
    /// Use this method when you explicitly need caching (e.g., federation,
    /// expensive computations). Most FraiseQL deployments don't need this.
    ///
    /// # Example
    ///
    /// ```rust
    /// use fraiseql_core::cache::CacheConfig;
    ///
    /// let config = CacheConfig::enabled();
    /// assert!(config.enabled);
    /// assert_eq!(config.max_entries, 10_000);
    /// ```
    #[must_use]
    pub const fn enabled() -> Self {
        Self {
            enabled:            true,
            max_entries:        10_000,
            ttl_seconds:        86_400,
            cache_list_queries: true,
            rls_enforcement:    RlsEnforcement::Error,
            max_entry_bytes:    None,
            max_total_bytes:    None,
        }
    }

    /// Create cache configuration with caching disabled.
    ///
    /// This is now the **default behavior**. Use this method for explicit clarity
    /// or to override a previously enabled configuration.
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
            enabled:            false,
            max_entries:        10_000,
            ttl_seconds:        86_400,
            cache_list_queries: true,
            rls_enforcement:    RlsEnforcement::Error,
            max_entry_bytes:    None,
            max_total_bytes:    None,
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
    #![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

    use super::*;

    #[test]
    fn test_default_config() {
        let config = CacheConfig::default();
        assert!(!config.enabled); // Disabled by default as of rc.12
        assert_eq!(config.max_entries, 10_000);
        assert_eq!(config.ttl_seconds, 86_400);
        assert!(config.cache_list_queries);
    }

    #[test]
    fn test_with_max_entries() {
        let config = CacheConfig::with_max_entries(50_000);
        assert_eq!(config.max_entries, 50_000);
        assert!(!config.enabled); // Disabled by default as of rc.12
        assert_eq!(config.ttl_seconds, 86_400);
    }

    #[test]
    fn test_with_ttl() {
        let config = CacheConfig::with_ttl(3_600);
        assert_eq!(config.ttl_seconds, 3_600);
        assert!(!config.enabled); // Disabled by default as of rc.12
        assert_eq!(config.max_entries, 10_000);
    }

    #[test]
    fn test_enabled() {
        let config = CacheConfig::enabled();
        assert!(config.enabled);
        assert_eq!(config.max_entries, 10_000);
        assert_eq!(config.ttl_seconds, 86_400);
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
