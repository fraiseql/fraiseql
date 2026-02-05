//! Metrics configuration with SLO tracking.

use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct MetricsConfig {
    #[serde(default = "default_enabled")]
    pub enabled: bool,

    #[serde(default = "default_path")]
    pub path: String,

    #[serde(default = "default_format")]
    pub format: String,

    /// SLO configuration
    #[serde(default)]
    pub slos: SloConfig,
}

impl Default for MetricsConfig {
    fn default() -> Self {
        Self {
            enabled: default_enabled(),
            path:    default_path(),
            format:  default_format(),
            slos:    SloConfig::default(),
        }
    }
}

fn default_enabled() -> bool {
    true
}
fn default_path() -> String {
    "/metrics".to_string()
}
fn default_format() -> String {
    "prometheus".to_string()
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct SloConfig {
    /// Target latency percentiles to track
    #[serde(default = "default_latency_percentiles")]
    pub latency_percentiles: Vec<f64>,

    /// Latency SLO targets (p99 < Xms)
    #[serde(default)]
    pub latency_targets: LatencyTargets,

    /// Availability SLO target (e.g., 0.999 for 99.9%)
    #[serde(default = "default_availability_target")]
    pub availability_target: f64,

    /// Error rate SLO target (e.g., 0.01 for 1%)
    #[serde(default = "default_error_rate_target")]
    pub error_rate_target: f64,
}

fn default_latency_percentiles() -> Vec<f64> {
    vec![0.5, 0.9, 0.95, 0.99]
}
fn default_availability_target() -> f64 {
    0.999
}
fn default_error_rate_target() -> f64 {
    0.01
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct LatencyTargets {
    #[serde(default = "default_graphql_latency")]
    pub graphql_p99_ms: u64,

    #[serde(default = "default_webhook_latency")]
    pub webhook_p99_ms: u64,

    #[serde(default = "default_auth_latency")]
    pub auth_p99_ms: u64,

    #[serde(default = "default_file_upload_latency")]
    pub file_upload_p99_ms: u64,
}

fn default_graphql_latency() -> u64 {
    100
}
fn default_webhook_latency() -> u64 {
    500
}
fn default_auth_latency() -> u64 {
    10
}
fn default_file_upload_latency() -> u64 {
    2000
}
