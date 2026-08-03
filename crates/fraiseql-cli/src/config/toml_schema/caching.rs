//! Caching and analytics configuration for TOML schema.

use serde::{Deserialize, Serialize};

/// Declarative result-cache rules (#623).
///
/// Rules are **lowered at compile time** onto the two compiled fields the
/// runtime's result cache already consumes: the named query's
/// `cache_ttl_seconds` (the per-view TTL map) and each trigger mutation's
/// `invalidates_views` (mutation-driven invalidation). Runtime enablement
/// remains the server's `cache_enabled`.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct CachingConfig {
    /// Gate for the rules below: `true` with rules lowers them; `false` with
    /// rules — or `true` without rules — is refused at compile time.
    #[serde(default)]
    pub enabled:   bool,
    /// Cache backend. Only `"memory"` exists — the runtime result cache is
    /// in-process; any other value is refused at compile time.
    pub backend:   String,
    /// Refused at compile time: there is no Redis-backed result cache.
    pub redis_url: Option<String>,
    /// Per-query cache rules.
    pub rules:     Vec<CacheRule>,
}

impl Default for CachingConfig {
    fn default() -> Self {
        Self {
            enabled:   false,
            backend:   "memory".to_string(),
            redis_url: None,
            rules:     vec![],
        }
    }
}

/// One declarative cache rule (#623).
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CacheRule {
    /// Name of an existing query (compile error otherwise).
    pub query:                 String,
    /// TTL lowered onto the query's `cache_ttl_seconds`. A query that already
    /// declares one (SDK/JSON-authored) is a compile error — two authoring
    /// sources must not silently last-write-win.
    pub ttl_seconds:           u32,
    /// Names of existing mutations (compile error otherwise) whose success
    /// must invalidate this query's cached results; each gains the query's
    /// view in its `invalidates_views`.
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
