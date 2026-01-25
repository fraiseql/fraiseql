use std::{collections::HashMap, path::PathBuf};

use serde::Deserialize;

pub mod cors;
pub mod env;
pub mod loader;
pub mod metrics;
pub mod rate_limiting;
#[cfg(test)]
mod tests;
pub mod tracing;
pub mod validation;

// Re-export config types
pub use cors::CorsConfig;
pub use metrics::{LatencyTargets, MetricsConfig, SloConfig};
pub use rate_limiting::{BackpressureConfig, RateLimitRule, RateLimitingConfig};
pub use tracing::TracingConfig;

/// Root configuration structure
#[derive(Debug, Clone, Deserialize)]
pub struct RuntimeConfig {
    pub server:   ServerConfig,
    pub database: DatabaseConfig,

    #[serde(default)]
    pub webhooks: HashMap<String, WebhookConfig>,

    #[serde(default)]
    pub files: HashMap<String, FileConfig>,

    #[serde(default)]
    pub auth: Option<AuthConfig>,

    #[serde(default)]
    pub notifications: Option<NotificationsConfig>,

    #[serde(default)]
    pub observers: HashMap<String, ObserverConfig>,

    #[serde(default)]
    pub interceptors: HashMap<String, Vec<String>>,

    #[serde(default)]
    pub rate_limiting: Option<RateLimitingConfig>,

    #[serde(default)]
    pub cors: Option<CorsConfig>,

    #[serde(default)]
    pub metrics: Option<MetricsConfig>,

    #[serde(default)]
    pub tracing: Option<TracingConfig>,

    #[serde(default)]
    pub logging: Option<LoggingConfig>,

    #[serde(default)]
    pub storage: HashMap<String, StorageConfig>,

    #[serde(default)]
    pub search: Option<SearchConfig>,

    #[serde(default)]
    pub cache: Option<CacheConfig>,

    #[serde(default)]
    pub queues: Option<QueueConfig>,

    #[serde(default)]
    pub realtime: Option<RealtimeConfig>,

    #[serde(default)]
    pub custom_endpoints: Option<CustomEndpointsConfig>,

    #[serde(default)]
    pub lifecycle: Option<LifecycleConfig>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ServerConfig {
    #[serde(default = "default_port")]
    pub port: u16,

    #[serde(default = "default_host")]
    pub host: String,

    #[serde(default)]
    pub workers: Option<usize>,

    #[serde(default)]
    pub tls: Option<TlsConfig>,

    #[serde(default)]
    pub limits: Option<ServerLimitsConfig>,
}

fn default_port() -> u16 {
    4000
}
fn default_host() -> String {
    "127.0.0.1".to_string()
}

#[derive(Debug, Clone, Deserialize)]
pub struct TlsConfig {
    pub cert_file: PathBuf,
    pub key_file:  PathBuf,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ServerLimitsConfig {
    #[serde(default = "default_max_request_size")]
    pub max_request_size: String,

    #[serde(default = "default_request_timeout")]
    pub request_timeout: String,

    #[serde(default = "default_max_concurrent")]
    pub max_concurrent_requests: usize,

    #[serde(default = "default_max_queue_depth")]
    pub max_queue_depth: usize,
}

fn default_max_request_size() -> String {
    "10MB".to_string()
}
fn default_request_timeout() -> String {
    "30s".to_string()
}
fn default_max_concurrent() -> usize {
    1000
}
fn default_max_queue_depth() -> usize {
    5000
}

#[derive(Debug, Clone, Deserialize)]
pub struct DatabaseConfig {
    pub url_env: String,

    #[serde(default = "default_pool_size")]
    pub pool_size: u32,

    #[serde(default)]
    pub pool_timeout: Option<String>,

    #[serde(default)]
    pub statement_timeout: Option<String>,

    #[serde(default)]
    pub replicas: Vec<ReplicaConfig>,

    #[serde(default)]
    pub health_check_interval: Option<String>,
}

fn default_pool_size() -> u32 {
    10
}

#[derive(Debug, Clone, Deserialize)]
pub struct ReplicaConfig {
    pub url_env: String,

    #[serde(default = "default_weight")]
    pub weight: u32,
}

fn default_weight() -> u32 {
    1
}

/// Lifecycle configuration for graceful shutdown
#[derive(Debug, Clone, Deserialize)]
pub struct LifecycleConfig {
    /// Time to wait for in-flight requests to complete
    #[serde(default = "default_shutdown_timeout")]
    pub shutdown_timeout: String,

    /// Time to wait before starting shutdown (for load balancer deregistration)
    #[serde(default = "default_shutdown_delay")]
    pub shutdown_delay: String,

    /// Health check endpoint path
    #[serde(default = "default_health_path")]
    pub health_path: String,

    /// Readiness check endpoint path
    #[serde(default = "default_ready_path")]
    pub ready_path: String,
}

impl Default for LifecycleConfig {
    fn default() -> Self {
        Self {
            shutdown_timeout: default_shutdown_timeout(),
            shutdown_delay:   default_shutdown_delay(),
            health_path:      default_health_path(),
            ready_path:       default_ready_path(),
        }
    }
}

fn default_shutdown_timeout() -> String {
    "30s".to_string()
}
fn default_shutdown_delay() -> String {
    "5s".to_string()
}
fn default_health_path() -> String {
    "/health".to_string()
}
fn default_ready_path() -> String {
    "/ready".to_string()
}

// Placeholder structs for future phases (TODO: will be defined in later phases)

#[derive(Debug, Clone, Deserialize)]
pub struct WebhookConfig {
    pub secret_env: String,
    pub provider:   String,
    #[serde(default)]
    pub path:       Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct FileConfig {
    pub storage:  String,
    pub max_size: String,
    #[serde(default)]
    pub path:     Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AuthConfig {
    pub jwt:               JwtConfig,
    #[serde(default)]
    pub providers:         HashMap<String, OAuthProviderConfig>,
    #[serde(default)]
    pub callback_base_url: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct JwtConfig {
    pub secret_env: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct OAuthProviderConfig {
    pub provider_type:     String,
    pub client_id_env:     String,
    pub client_secret_env: String,
    #[serde(default)]
    pub issuer_url:        Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct NotificationsConfig {
    // TODO: Phase 6
}

#[derive(Debug, Clone, Deserialize)]
pub struct ObserverConfig {
    pub entity:  String,
    pub events:  Vec<String>,
    pub actions: Vec<ActionConfig>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ActionConfig {
    #[serde(rename = "type")]
    pub action_type: String,
    #[serde(default)]
    pub template:    Option<String>,
}

// These types are now defined in their own modules and re-exported above

#[derive(Debug, Clone, Deserialize)]
pub struct LoggingConfig {
    // TODO: Phase 2
}

#[derive(Debug, Clone, Deserialize)]
pub struct StorageConfig {
    pub backend: String,
    #[serde(default)]
    pub bucket:  Option<String>,
    #[serde(default)]
    pub path:    Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SearchConfig {
    // TODO: Phase 9
}

#[derive(Debug, Clone, Deserialize)]
pub struct CacheConfig {
    // TODO: Phase 2
}

#[derive(Debug, Clone, Deserialize)]
pub struct QueueConfig {
    // TODO: Phase 10
}

#[derive(Debug, Clone, Deserialize)]
pub struct RealtimeConfig {
    // TODO: Phase 11
}

#[derive(Debug, Clone, Deserialize)]
pub struct CustomEndpointsConfig {
    // TODO: Phase 12
}
