//! Redis configuration for deduplication and caching.

use std::env;

use serde::{Deserialize, Serialize};

use crate::error::{ObserverError, Result};

/// Redis configuration for deduplication and caching
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RedisConfig {
    /// Redis connection URL (e.g., "redis://localhost:6379")
    #[serde(default = "default_redis_url")]
    pub url: String,

    /// Maximum number of connections in pool (default: 10)
    #[serde(default = "default_redis_pool_size")]
    pub pool_size: usize,

    /// Connection timeout in seconds (default: 5)
    #[serde(default = "default_redis_connect_timeout_secs")]
    pub connect_timeout_secs: u64,

    /// Command timeout in seconds (default: 2)
    #[serde(default = "default_redis_command_timeout_secs")]
    pub command_timeout_secs: u64,

    /// Deduplication window in seconds (default: 300 = 5 minutes)
    #[serde(default = "default_dedup_window_secs")]
    pub dedup_window_secs: u64,

    /// Cache TTL in seconds (default: 60)
    #[serde(default = "default_cache_ttl_secs")]
    pub cache_ttl_secs: u64,
}

fn default_redis_url() -> String {
    env::var("FRAISEQL_REDIS_URL").unwrap_or_else(|_| "redis://localhost:6379".to_string())
}

const fn default_redis_pool_size() -> usize {
    10
}

const fn default_redis_connect_timeout_secs() -> u64 {
    5
}

const fn default_redis_command_timeout_secs() -> u64 {
    2
}

const fn default_dedup_window_secs() -> u64 {
    300 // 5 minutes
}

const fn default_cache_ttl_secs() -> u64 {
    60 // 1 minute
}

impl Default for RedisConfig {
    fn default() -> Self {
        Self {
            url:                  default_redis_url(),
            pool_size:            default_redis_pool_size(),
            connect_timeout_secs: default_redis_connect_timeout_secs(),
            command_timeout_secs: default_redis_command_timeout_secs(),
            dedup_window_secs:    default_dedup_window_secs(),
            cache_ttl_secs:       default_cache_ttl_secs(),
        }
    }
}

impl RedisConfig {
    /// Apply environment variable overrides
    #[must_use]
    pub fn with_env_overrides(mut self) -> Self {
        if let Ok(url) = env::var("FRAISEQL_REDIS_URL") {
            self.url = url;
        }
        if let Ok(v) = env::var("FRAISEQL_REDIS_POOL_SIZE") {
            if let Ok(size) = v.parse() {
                self.pool_size = size;
            }
        }
        if let Ok(v) = env::var("FRAISEQL_REDIS_CONNECT_TIMEOUT_SECS") {
            if let Ok(secs) = v.parse() {
                self.connect_timeout_secs = secs;
            }
        }
        if let Ok(v) = env::var("FRAISEQL_REDIS_COMMAND_TIMEOUT_SECS") {
            if let Ok(secs) = v.parse() {
                self.command_timeout_secs = secs;
            }
        }
        if let Ok(v) = env::var("FRAISEQL_REDIS_DEDUP_WINDOW_SECS") {
            if let Ok(secs) = v.parse() {
                self.dedup_window_secs = secs;
            }
        }
        if let Ok(v) = env::var("FRAISEQL_REDIS_CACHE_TTL_SECS") {
            if let Ok(secs) = v.parse() {
                self.cache_ttl_secs = secs;
            }
        }
        self
    }

    /// Validate the configuration
    pub fn validate(&self) -> Result<()> {
        if self.url.is_empty() {
            return Err(ObserverError::InvalidConfig {
                message: "redis.url cannot be empty".to_string(),
            });
        }
        if self.pool_size == 0 {
            return Err(ObserverError::InvalidConfig {
                message: "redis.pool_size must be > 0".to_string(),
            });
        }
        if self.connect_timeout_secs == 0 {
            return Err(ObserverError::InvalidConfig {
                message: "redis.connect_timeout_secs must be > 0".to_string(),
            });
        }
        if self.command_timeout_secs == 0 {
            return Err(ObserverError::InvalidConfig {
                message: "redis.command_timeout_secs must be > 0".to_string(),
            });
        }
        if self.dedup_window_secs == 0 || self.dedup_window_secs > 3600 {
            return Err(ObserverError::InvalidConfig {
                message: "redis.dedup_window_secs must be between 1 and 3600".to_string(),
            });
        }
        if self.cache_ttl_secs == 0 || self.cache_ttl_secs > 3600 {
            return Err(ObserverError::InvalidConfig {
                message: "redis.cache_ttl_secs must be between 1 and 3600".to_string(),
            });
        }
        Ok(())
    }
}
