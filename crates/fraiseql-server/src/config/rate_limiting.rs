//! Rate limiting configuration with backpressure support.

use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct RateLimitingConfig {
    #[serde(default = "default_enabled")]
    pub enabled: bool,

    /// Default rate limit (e.g., "100/minute")
    pub default: String,

    /// Storage backend: memory, redis
    #[serde(default = "default_backend")]
    pub backend: String,

    /// Redis URL (if using redis storage)
    pub redis_url_env: Option<String>,

    /// Custom rules
    #[serde(default)]
    pub rules: Vec<RateLimitRule>,

    /// Backpressure configuration
    #[serde(default)]
    pub backpressure: BackpressureConfig,
}

fn default_enabled() -> bool {
    true
}
fn default_backend() -> String {
    "memory".to_string()
}

#[derive(Debug, Clone, Deserialize)]
pub struct RateLimitRule {
    /// Match by path pattern (e.g., "/auth/*")
    pub path: Option<String>,

    /// Match by mutation name
    pub mutation: Option<String>,

    /// Match by query name
    pub query: Option<String>,

    /// Limit (e.g., "10/minute", "100/hour")
    pub limit: String,

    /// Key extraction: ip, user, api_key, composite
    #[serde(default = "default_key_by")]
    pub by: String,

    /// Burst allowance (requests above limit that can be queued)
    #[serde(default)]
    pub burst: Option<u32>,
}

fn default_key_by() -> String {
    "ip".to_string()
}

#[derive(Debug, Clone, Deserialize)]
pub struct BackpressureConfig {
    /// Enable request queuing when at limit
    #[serde(default)]
    pub queue_enabled: bool,

    /// Maximum queue size per key
    #[serde(default = "default_queue_size")]
    pub max_queue_size: usize,

    /// Maximum time to wait in queue
    #[serde(default = "default_queue_timeout")]
    pub queue_timeout: String,

    /// Shed load when queue is full (503 vs queue)
    #[serde(default = "default_load_shed")]
    pub load_shed: bool,
}

impl Default for BackpressureConfig {
    fn default() -> Self {
        Self {
            queue_enabled:  false,
            max_queue_size: default_queue_size(),
            queue_timeout:  default_queue_timeout(),
            load_shed:      default_load_shed(),
        }
    }
}

fn default_queue_size() -> usize {
    100
}
fn default_queue_timeout() -> String {
    "5s".to_string()
}
fn default_load_shed() -> bool {
    true
}
