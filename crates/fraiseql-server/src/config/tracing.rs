//! Tracing and logging configuration.

use serde::Deserialize;

/// Distributed-tracing and structured-logging configuration.
#[derive(Debug, Clone, Deserialize)]
pub struct TracingConfig {
    /// Whether tracing/logging is active.  Default: `true`.
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

    /// OTLP exporter endpoint.
    ///
    /// When set (e.g. `"http://otel-collector:4317"`), the server initializes an
    /// `OpenTelemetry` OTLP exporter and pipes `tracing` spans to it.
    /// When `None`, the `OTEL_EXPORTER_OTLP_ENDPOINT` environment variable is
    /// checked as a fallback. If neither is set, no OTLP export occurs and there
    /// is zero overhead (no gRPC connection attempt).
    #[serde(default)]
    pub otlp_endpoint: Option<String>,

    /// OTLP exporter timeout in seconds.
    ///
    /// Controls how long the OTLP HTTP exporter waits for a response from the
    /// collector before timing out. Defaults to 10 seconds.
    /// Override via `[tracing] otlp_export_timeout_secs = 30` in `fraiseql.toml`.
    #[serde(default = "default_otlp_timeout_secs")]
    pub otlp_export_timeout_secs: u64,
}

impl Default for TracingConfig {
    fn default() -> Self {
        Self {
            enabled:                  default_enabled(),
            level:                    default_level(),
            format:                   default_format(),
            service_name:             default_service_name(),
            otlp_endpoint:            None,
            otlp_export_timeout_secs: default_otlp_timeout_secs(),
        }
    }
}

const fn default_enabled() -> bool {
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
const fn default_otlp_timeout_secs() -> u64 {
    10
}
