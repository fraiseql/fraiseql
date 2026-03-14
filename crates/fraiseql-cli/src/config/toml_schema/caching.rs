//! Caching and analytics configuration for TOML schema.

use serde::{Deserialize, Serialize};

/// Caching configuration
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct CachingConfig {
    /// Enable caching
    #[serde(default)]
    pub enabled:   bool,
    /// Cache backend (redis, memory, postgresql)
    pub backend:   String,
    /// Redis connection URL
    pub redis_url: Option<String>,
    /// Cache invalidation rules
    pub rules:     Vec<CacheRule>,
}

impl Default for CachingConfig {
    fn default() -> Self {
        Self {
            enabled:   false,
            backend:   "redis".to_string(),
            redis_url: None,
            rules:     vec![],
        }
    }
}

/// Cache invalidation rule
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CacheRule {
    /// Query pattern to cache
    pub query:                 String,
    /// Time-to-live in seconds
    pub ttl_seconds:           u32,
    /// Events that trigger cache invalidation
    pub invalidation_triggers: Vec<String>,
}

/// Analytics configuration
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct AnalyticsConfig {
    /// Enable analytics
    #[serde(default)]
    pub enabled: bool,
    /// Analytics queries
    pub queries: Vec<AnalyticsQuery>,
}

/// Analytics query definition
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AnalyticsQuery {
    /// Query name
    pub name:        String,
    /// SQL source for the query
    pub sql_source:  String,
    /// Query description
    pub description: Option<String>,
}
