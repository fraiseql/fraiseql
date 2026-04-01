//! Caching configuration.

use serde::{Deserialize, Serialize};

/// Caching configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct CacheConfig {
    /// Enable Automatic Persisted Queries (APQ).
    pub apq_enabled: bool,

    /// APQ cache TTL in seconds.
    pub apq_ttl_secs: u64,

    /// Maximum APQ cache entries.
    pub apq_max_entries: usize,

    /// Enable response caching.
    pub response_cache_enabled: bool,

    /// Response cache TTL in seconds.
    pub response_cache_ttl_secs: u64,

    /// Maximum response cache entries.
    pub response_cache_max_entries: usize,
}

impl Default for CacheConfig {
    fn default() -> Self {
        Self {
            apq_enabled: true,
            apq_ttl_secs: 86400, // 24 hours
            apq_max_entries: 10_000,
            response_cache_enabled: false,
            response_cache_ttl_secs: 60,
            response_cache_max_entries: 1_000,
        }
    }
}
