//! Observability configuration for TOML schema.

use serde::{Deserialize, Serialize};

/// Observability configuration
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct ObservabilityConfig {
    /// Enable Prometheus metrics
    pub prometheus_enabled:            bool,
    /// Port for Prometheus metrics endpoint
    pub prometheus_port:               u16,
    /// Enable OpenTelemetry tracing
    pub otel_enabled:                  bool,
    /// OpenTelemetry exporter type
    pub otel_exporter:                 String,
    /// Jaeger endpoint for trace collection
    pub otel_jaeger_endpoint:          Option<String>,
    /// Enable health check endpoint
    pub health_check_enabled:          bool,
    /// Health check interval in seconds
    pub health_check_interval_seconds: u32,
    /// Log level threshold
    pub log_level:                     String,
    /// Log output format (json, text)
    pub log_format:                    String,
}

impl Default for ObservabilityConfig {
    fn default() -> Self {
        Self {
            prometheus_enabled:            false,
            prometheus_port:               9090,
            otel_enabled:                  false,
            otel_exporter:                 "jaeger".to_string(),
            otel_jaeger_endpoint:          None,
            health_check_enabled:          true,
            health_check_interval_seconds: 30,
            log_level:                     "info".to_string(),
            log_format:                    "json".to_string(),
        }
    }
}
