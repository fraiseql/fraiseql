//! Tracing and logging configuration.

use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct TracingConfig {
    #[serde(default = "default_enabled")]
    pub enabled: bool,

    /// Log level filter
    #[serde(default = "default_level")]
    pub level: String,

    /// Log format: json, pretty
    #[serde(default = "default_format")]
    pub format: String,

    /// Service name for distributed tracing
    #[serde(default = "default_service_name")]
    pub service_name: String,
}

impl Default for TracingConfig {
    fn default() -> Self {
        Self {
            enabled: default_enabled(),
            level: default_level(),
            format: default_format(),
            service_name: default_service_name(),
        }
    }
}

fn default_enabled() -> bool {
    true
}
fn default_level() -> String {
    "info".to_string()
}
fn default_format() -> String {
    "json".to_string()
}
fn default_service_name() -> String {
    "fraiseql".to_string()
}
