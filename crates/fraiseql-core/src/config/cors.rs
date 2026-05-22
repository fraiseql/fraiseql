//! Cross-Origin Resource Sharing (CORS) configuration.

use serde::{Deserialize, Serialize};

/// Cross-Origin Resource Sharing (CORS) configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct CorsConfig {
    /// Enabled CORS.
    pub enabled: bool,

    /// Allowed origins. Empty = allow all, "*" = allow any.
    pub allowed_origins: Vec<String>,

    /// Allowed HTTP methods.
    pub allowed_methods: Vec<String>,

    /// Allowed headers.
    pub allowed_headers: Vec<String>,

    /// Headers to expose to the client.
    pub expose_headers: Vec<String>,

    /// Allow credentials (cookies, authorization headers).
    pub allow_credentials: bool,

    /// Preflight cache duration in seconds.
    pub max_age_secs: u64,
}

impl Default for CorsConfig {
    fn default() -> Self {
        Self {
            enabled:           true,
            allowed_origins:   vec![], // Empty = allow all
            allowed_methods:   vec!["GET".to_string(), "POST".to_string(), "OPTIONS".to_string()],
            allowed_headers:   vec![
                "Content-Type".to_string(),
                "Authorization".to_string(),
                "X-Request-ID".to_string(),
            ],
            expose_headers:    vec![],
            allow_credentials: false,
            max_age_secs:      86400, // 24 hours
        }
    }
}
