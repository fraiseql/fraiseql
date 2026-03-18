//! Default value functions for [`super::ServerConfig`] serde fields.

use std::{net::SocketAddr, path::PathBuf};

pub fn default_schema_path() -> PathBuf {
    PathBuf::from("schema.compiled.json")
}

pub fn default_database_url() -> String {
    "postgresql://localhost/fraiseql".to_string()
}

#[cfg(feature = "arrow")]
pub fn default_flight_bind_addr() -> SocketAddr {
    std::env::var("FRAISEQL_FLIGHT_BIND_ADDR")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or_else(|| "0.0.0.0:50051".parse().expect("valid Flight bind address"))
}

pub fn default_bind_addr() -> SocketAddr {
    "127.0.0.1:8000".parse().expect("hard-coded addr literal is always valid")
}

pub const fn default_true() -> bool {
    true
}

/// 1 MB default body limit.
pub const fn default_max_request_body_bytes() -> usize {
    1_048_576
}

/// 100 KiB default GET query size limit.
pub const fn default_max_get_query_bytes() -> usize {
    100_000
}

/// Default maximum number of HTTP headers per request.
///
/// Prevents header-flooding DoS attacks that exhaust memory by sending
/// thousands of unique headers.
pub const fn default_max_header_count() -> usize {
    100
}

/// Default maximum total size of all HTTP headers in bytes (32 KiB).
///
/// Prevents memory exhaustion from oversized header values (e.g. huge cookies
/// or authorization tokens).
pub const fn default_max_header_bytes() -> usize {
    32_768
}

pub fn default_graphql_path() -> String {
    "/graphql".to_string()
}

pub fn default_health_path() -> String {
    "/health".to_string()
}

pub fn default_readiness_path() -> String {
    "/readiness".to_string()
}

pub const fn default_shutdown_timeout_secs() -> u64 {
    30
}

pub fn default_introspection_path() -> String {
    "/introspection".to_string()
}

pub fn default_metrics_path() -> String {
    "/metrics".to_string()
}

pub fn default_metrics_json_path() -> String {
    "/metrics/json".to_string()
}

pub fn default_playground_path() -> String {
    "/playground".to_string()
}

pub fn default_subscription_path() -> String {
    "/ws".to_string()
}

pub const fn default_pool_min_size() -> usize {
    5
}

pub const fn default_pool_max_size() -> usize {
    20
}

pub const fn default_pool_timeout() -> u64 {
    30
}

pub fn default_tls_min_version() -> String {
    "1.2".to_string()
}

pub fn default_postgres_ssl_mode() -> String {
    "prefer".to_string()
}

pub const fn default_redis_ssl() -> bool {
    false
}

pub const fn default_clickhouse_https() -> bool {
    false
}

pub const fn default_elasticsearch_https() -> bool {
    false
}

pub const fn default_verify_certs() -> bool {
    true
}

pub const fn default_otlp_timeout_secs() -> u64 {
    10
}

pub fn default_service_name() -> String {
    "fraiseql".to_string()
}
