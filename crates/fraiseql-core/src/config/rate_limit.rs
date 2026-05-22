//! Rate limiting configuration.

use serde::{Deserialize, Serialize};

/// Rate limiting configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct RateLimitConfig {
    /// Enable rate limiting.
    pub enabled: bool,

    /// Maximum requests per window.
    pub requests_per_window: u32,

    /// Window duration in seconds.
    pub window_secs: u64,

    /// Key extractor (ip, user, `api_key`).
    pub key_by: RateLimitKey,

    /// Paths to exclude from rate limiting.
    pub exclude_paths: Vec<String>,

    /// Custom limits per path pattern.
    pub path_limits: Vec<PathRateLimit>,
}

impl Default for RateLimitConfig {
    fn default() -> Self {
        Self {
            enabled:             false,
            requests_per_window: 100,
            window_secs:         60,
            key_by:              RateLimitKey::Ip,
            exclude_paths:       vec!["/health".to_string()],
            path_limits:         vec![],
        }
    }
}

/// Rate limit key extractor.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum RateLimitKey {
    /// Rate limit by IP address.
    #[default]
    Ip,
    /// Rate limit by authenticated user.
    User,
    /// Rate limit by API key.
    ApiKey,
}

/// Per-path rate limit override.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PathRateLimit {
    /// Path pattern (glob).
    pub path:                String,
    /// Maximum requests per window for this path.
    pub requests_per_window: u32,
}
