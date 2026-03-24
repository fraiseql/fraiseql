//! Performance optimization feature flags.

use std::env;

use serde::{Deserialize, Serialize};

use crate::error::{ObserverError, Result};

/// Performance optimization features
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceConfig {
    /// Enable Redis-based event deduplication (requires redis config)
    #[serde(default)]
    pub enable_dedup: bool,

    /// Enable Redis-based action result caching (requires redis config)
    #[serde(default)]
    pub enable_caching: bool,

    /// Enable concurrent action execution within observers
    #[serde(default = "default_true")]
    pub enable_concurrent: bool,

    /// Maximum concurrent actions per observer (default: 10)
    #[serde(default = "default_max_concurrent_actions")]
    pub max_concurrent_actions: usize,

    /// Concurrent execution timeout in milliseconds (default: 30000)
    #[serde(default = "default_concurrent_timeout_ms")]
    pub concurrent_timeout_ms: u64,
}

const fn default_true() -> bool {
    true
}

const fn default_max_concurrent_actions() -> usize {
    10
}

const fn default_concurrent_timeout_ms() -> u64 {
    30000 // 30 seconds
}

impl Default for PerformanceConfig {
    fn default() -> Self {
        Self {
            enable_dedup:           false,
            enable_caching:         false,
            enable_concurrent:      true,
            max_concurrent_actions: default_max_concurrent_actions(),
            concurrent_timeout_ms:  default_concurrent_timeout_ms(),
        }
    }
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
        if let Ok(v) = env::var("FRAISEQL_ENABLE_CONCURRENT") {
            self.enable_concurrent = v.eq_ignore_ascii_case("true") || v == "1";
        }
        if let Ok(v) = env::var("FRAISEQL_MAX_CONCURRENT_ACTIONS") {
            if let Ok(max) = v.parse() {
                self.max_concurrent_actions = max;
            }
        }
        if let Ok(v) = env::var("FRAISEQL_CONCURRENT_TIMEOUT_MS") {
            if let Ok(ms) = v.parse() {
                self.concurrent_timeout_ms = ms;
            }
        }
        self
    }

    /// Validate the configuration
    ///
    /// # Errors
    ///
    /// Returns an error if the operation fails.
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
        if self.max_concurrent_actions == 0 {
            return Err(ObserverError::InvalidConfig {
                message: "performance.max_concurrent_actions must be > 0".to_string(),
            });
        }
        if self.concurrent_timeout_ms == 0 {
            return Err(ObserverError::InvalidConfig {
                message: "performance.concurrent_timeout_ms must be > 0".to_string(),
            });
        }
        Ok(())
    }
}
