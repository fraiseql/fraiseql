//! CORS configuration.

use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct CorsConfig {
    #[serde(default = "default_enabled")]
    pub enabled: bool,

    /// Allowed origins (supports wildcards like "https://*.example.com")
    #[serde(default = "default_origins")]
    pub origins: Vec<String>,

    /// Allowed HTTP methods
    #[serde(default = "default_methods")]
    pub methods: Vec<String>,

    /// Allowed headers
    #[serde(default = "default_headers")]
    pub headers: Vec<String>,

    /// Allow credentials
    #[serde(default)]
    pub credentials: bool,

    /// Preflight cache duration in seconds
    #[serde(default = "default_max_age")]
    pub max_age: u64,

    /// Exposed headers (returned to browser)
    #[serde(default)]
    pub expose_headers: Vec<String>,

    /// Allow private network access (for localhost development)
    #[serde(default)]
    pub private_network: bool,
}

impl Default for CorsConfig {
    fn default() -> Self {
        Self {
            enabled:         default_enabled(),
            origins:         default_origins(),
            methods:         default_methods(),
            headers:         default_headers(),
            credentials:     false,
            max_age:         default_max_age(),
            expose_headers:  Vec::new(),
            private_network: false,
        }
    }
}

fn default_enabled() -> bool {
    true
}
fn default_origins() -> Vec<String> {
    vec!["*".to_string()]
}
fn default_methods() -> Vec<String> {
    vec!["GET".to_string(), "POST".to_string(), "OPTIONS".to_string()]
}
fn default_headers() -> Vec<String> {
    vec![
        "Authorization".to_string(),
        "Content-Type".to_string(),
        "X-Request-ID".to_string(),
    ]
}
fn default_max_age() -> u64 {
    86400
}
