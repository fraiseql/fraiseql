//! Performance optimization feature flags.

use std::env;

use serde::{Deserialize, Serialize};

use crate::error::{ObserverError, Result};

/// Performance optimization features.
///
/// Strict (`deny_unknown_fields`): an unrecognised key fails the parse. It used to accept
/// `enable_concurrent`, `max_concurrent_actions` and `concurrent_timeout_ms`, which nothing
/// read, so `enable_concurrent = true` ran actions one after another and said nothing
/// (#1451). Those keys are gone, and a config that still sets one is refused.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PerformanceConfig {
    /// Enable Redis-based event deduplication (requires redis config)
    #[serde(default)]
    pub enable_dedup: bool,

    /// Enable Redis-based action result caching (requires redis config)
    #[serde(default)]
    pub enable_caching: bool,
}

impl PerformanceConfig {
    /// Apply environment variable overrides
    #[must_use]
    pub fn with_env_overrides(mut self) -> Self {
        if let Ok(v) = env::var("FRAISEQL_ENABLE_DEDUP") {
            self.enable_dedup = v.eq_ignore_ascii_case("true") || v == "1";
        }
        if let Ok(v) = env::var("FRAISEQL_ENABLE_CACHING") {
            self.enable_caching = v.eq_ignore_ascii_case("true") || v == "1";
        }
        self
    }

    /// Validate the configuration
    ///
    /// # Errors
    ///
    /// Returns [`ObserverError::InvalidConfig`] if dedup or caching is enabled without
    /// Redis.
    pub fn validate(&self, redis_configured: bool) -> Result<()> {
        // Dedup requires Redis
        if self.enable_dedup && !redis_configured {
            return Err(ObserverError::InvalidConfig {
                message: "performance.enable_dedup=true requires redis configuration".to_string(),
            });
        }
        // Caching requires Redis
        if self.enable_caching && !redis_configured {
            return Err(ObserverError::InvalidConfig {
                message: "performance.enable_caching=true requires redis configuration".to_string(),
            });
        }
        Ok(())
    }
}
