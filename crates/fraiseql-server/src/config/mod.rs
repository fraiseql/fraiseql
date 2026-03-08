//! Runtime configuration types for the FraiseQL server.
//!
//! Structs in this module are deserialized from `fraiseql.toml` (via the
//! `loader` sub-module) or assembled from environment variables (via the
//! `env` sub-module).
//! Sub-modules contain configuration for specific subsystems such as CORS,
//! metrics, rate limiting, and TLS.

use std::{collections::HashMap, path::PathBuf};

use serde::Deserialize;

pub mod cors;
pub mod env;
pub mod error_sanitization;
pub mod loader;
pub mod metrics;
pub mod pool_tuning;
pub mod rate_limiting;
#[cfg(test)]
mod tests;
pub mod tracing;
pub mod validation;

// Re-export config types
pub use cors::CorsConfig;
pub use error_sanitization::{ErrorSanitizationConfig, ErrorSanitizer};
pub use metrics::{LatencyTargets, MetricsConfig, SloConfig};
pub use pool_tuning::PoolTuningConfig;
pub use rate_limiting::{BackpressureConfig, RateLimitRule, RateLimitingConfig};
pub use tracing::TracingConfig;

/// Root configuration structure loaded from `fraiseql.toml`.
#[derive(Debug, Clone, Deserialize)]
pub struct RuntimeConfig {
    /// HTTP server binding, TLS, and connection-limit settings.
    pub server:   HttpServerConfig,
    /// Primary database connection and pool settings.
    pub database: DatabaseConfig,

    /// Named webhook route configurations, keyed by route name.
    #[serde(default)]
    pub webhooks: HashMap<String, WebhookRouteConfig>,

    /// Named file-upload route configurations, keyed by route name.
    #[serde(default)]
    pub files: HashMap<String, FileConfig>,

    /// Optional JWT authentication and OAuth provider configuration.
    #[serde(default)]
    pub auth: Option<AuthConfig>,

    /// Reserved: placeholder for future notification system configuration.
    #[serde(default)]
    pub notifications: Option<NotificationsConfig>,

    /// Event observer configurations, keyed by observer name.
    #[serde(default)]
    pub observers: HashMap<String, ObserverConfig>,

    /// Request interceptor chains, keyed by interceptor name.
    #[serde(default)]
    pub interceptors: HashMap<String, Vec<String>>,

    /// Optional rate-limiting rules and backpressure thresholds.
    #[serde(default)]
    pub rate_limiting: Option<RateLimitingConfig>,

    /// Optional CORS origin and header policy.
    #[serde(default)]
    pub cors: Option<CorsConfig>,

    /// Optional Prometheus metrics and SLO tracking configuration.
    #[serde(default)]
    pub metrics: Option<MetricsConfig>,

    /// Optional distributed-tracing (OTLP/Jaeger) configuration.
    #[serde(default)]
    pub tracing: Option<TracingConfig>,

    /// Optional structured-logging configuration.
    #[serde(default)]
    pub logging: Option<LoggingConfig>,

    /// Named object-storage backend configurations, keyed by storage name.
    #[serde(default)]
    pub storage: HashMap<String, StorageConfig>,

    /// Reserved: placeholder for future search-indexing configuration.
    #[serde(default)]
    pub search: Option<SearchConfig>,

    /// Reserved: placeholder for future advanced caching strategy configuration.
    #[serde(default)]
    pub cache: Option<CacheConfig>,

    /// Reserved: placeholder for future job-queue configuration.
    #[serde(default)]
    pub queues: Option<QueueConfig>,

    /// Reserved: placeholder for future real-time update configuration.
    #[serde(default)]
    pub realtime: Option<RealtimeConfig>,

    /// Reserved: placeholder for future custom-endpoint configuration.
    #[serde(default)]
    pub custom_endpoints: Option<CustomEndpointsConfig>,

    /// Graceful-shutdown timing and health-check endpoint paths.
    #[serde(default)]
    pub lifecycle: Option<LifecycleConfig>,

}

/// HTTP server binding configuration.
#[derive(Debug, Clone, Deserialize)]
pub struct HttpServerConfig {
    /// TCP port to listen on.  Default: `4000`.
    #[serde(default = "default_port")]
    pub port: u16,

    /// Network interface to bind.  Default: `"127.0.0.1"`.
    #[serde(default = "default_host")]
    pub host: String,

    /// Number of async worker threads.  `None` uses the Tokio default (number of CPU cores).
    #[serde(default)]
    pub workers: Option<usize>,

    /// Optional TLS certificate and private key paths.
    #[serde(default)]
    pub tls: Option<TlsConfig>,

    /// Optional per-request and concurrency limits.
    #[serde(default)]
    pub limits: Option<ServerLimitsConfig>,
}

const fn default_port() -> u16 {
    4000
}
fn default_host() -> String {
    "127.0.0.1".to_string()
}

/// TLS certificate and private key paths for HTTPS listeners.
#[derive(Debug, Clone, Deserialize)]
pub struct TlsConfig {
    /// Path to the PEM-encoded TLS certificate (or certificate chain).
    pub cert_file: PathBuf,
    /// Path to the PEM-encoded private key corresponding to `cert_file`.
    pub key_file:  PathBuf,
}

/// Per-request body size and concurrency limits for the HTTP server.
#[derive(Debug, Clone, Deserialize)]
pub struct ServerLimitsConfig {
    /// Maximum allowed request body size as a human-readable string (e.g. `"10MB"`).  Default: `"10MB"`.
    #[serde(default = "default_max_request_size")]
    pub max_request_size: String,

    /// Maximum time to process a single request (e.g. `"30s"`).  Default: `"30s"`.
    #[serde(default = "default_request_timeout")]
    pub request_timeout: String,

    /// Maximum number of requests being processed simultaneously.  Default: `1000`.
    #[serde(default = "default_max_concurrent")]
    pub max_concurrent_requests: usize,

    /// Maximum number of requests waiting in the accept queue.  Default: `5000`.
    #[serde(default = "default_max_queue_depth")]
    pub max_queue_depth: usize,
}

fn default_max_request_size() -> String {
    "10MB".to_string()
}
fn default_request_timeout() -> String {
    "30s".to_string()
}
const fn default_max_concurrent() -> usize {
    1000
}
const fn default_max_queue_depth() -> usize {
    5000
}

/// Primary database connection and connection-pool configuration.
#[derive(Debug, Clone, Deserialize)]
pub struct DatabaseConfig {
    /// Name of the environment variable that holds the database connection URL.
    pub url_env: String,

    /// Maximum number of connections in the pool.  Default: `10`.
    #[serde(default = "default_pool_size")]
    pub pool_size: u32,

    /// How long to wait for an available connection before returning an error (e.g. `"5s"`).
    #[serde(default)]
    pub pool_timeout: Option<String>,

    /// Per-query statement timeout sent to the database (e.g. `"30s"`).
    #[serde(default)]
    pub statement_timeout: Option<String>,

    /// Optional read-replica pools used for load balancing SELECT queries.
    #[serde(default)]
    pub replicas: Vec<ReplicaConfig>,

    /// How often to ping the database to verify liveness (e.g. `"60s"`).
    #[serde(default)]
    pub health_check_interval: Option<String>,
}

const fn default_pool_size() -> u32 {
    10
}

/// Connection configuration for a single read replica.
#[derive(Debug, Clone, Deserialize)]
pub struct ReplicaConfig {
    /// Name of the environment variable that holds this replica's connection URL.
    pub url_env: String,

    /// Relative weight for load-balancing SELECT queries across replicas.  Default: `1`.
    #[serde(default = "default_weight")]
    pub weight: u32,
}

const fn default_weight() -> u32 {
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

/// Configuration for a single incoming webhook route.
#[derive(Debug, Clone, Deserialize)]
pub struct WebhookRouteConfig {
    /// Name of the environment variable that holds the webhook signing secret.
    pub secret_env: String,
    /// Webhook provider identifier (e.g. `"github"`, `"stripe"`).
    pub provider:   String,
    /// URL path override; if absent, the route name is used as the path segment.
    #[serde(default)]
    pub path:       Option<String>,
}

/// Configuration for a file-upload route.
#[derive(Debug, Clone, Deserialize)]
pub struct FileConfig {
    /// Named storage backend (must match a key in `storage`).
    pub storage:  String,
    /// Maximum upload size as a human-readable string (e.g. `"50MB"`).
    pub max_size: String,
    /// URL path prefix for upload and download endpoints.
    #[serde(default)]
    pub path:     Option<String>,
}

/// JWT authentication and OAuth provider configuration.
#[derive(Debug, Clone, Deserialize)]
pub struct AuthConfig {
    /// JWT signing secret configuration.
    pub jwt:               JwtConfig,
    /// Named OAuth2/OIDC provider configurations.
    #[serde(default)]
    pub providers:         HashMap<String, OAuthProviderConfig>,
    /// Base URL for OAuth callback endpoints (e.g. `"https://api.example.com"`).
    #[serde(default)]
    pub callback_base_url: Option<String>,
}

/// JWT signing-secret configuration.
#[derive(Debug, Clone, Deserialize)]
pub struct JwtConfig {
    /// Name of the environment variable that holds the JWT signing secret.
    pub secret_env: String,
}

/// Configuration for a single OAuth2/OIDC provider.
#[derive(Debug, Clone, Deserialize)]
pub struct OAuthProviderConfig {
    /// Well-known provider type identifier (e.g. `"auth0"`, `"github"`, `"google"`).
    pub provider_type:     String,
    /// Name of the environment variable that holds the OAuth client ID.
    pub client_id_env:     String,
    /// Name of the environment variable that holds the OAuth client secret.
    pub client_secret_env: String,
    /// OIDC issuer URL (required for providers that support OIDC discovery).
    #[serde(default)]
    pub issuer_url:        Option<String>,
}

/// Reserved: placeholder for future notification system configuration.
#[derive(Debug, Clone, Deserialize)]
pub struct NotificationsConfig {}

/// Configuration for a single event observer (entity-event → action).
#[derive(Debug, Clone, Deserialize)]
pub struct ObserverConfig {
    /// GraphQL entity type name to watch (e.g. `"User"`).
    pub entity:  String,
    /// List of mutation operation names that trigger this observer.
    pub events:  Vec<String>,
    /// Ordered list of actions to execute when an observed event fires.
    pub actions: Vec<ActionConfig>,
}

/// A single action within an observer pipeline.
#[derive(Debug, Clone, Deserialize)]
pub struct ActionConfig {
    /// Action type identifier (e.g. `"webhook"`, `"email"`, `"queue"`).
    #[serde(rename = "type")]
    pub action_type: String,
    /// Optional Jinja2-style template used to render the action payload.
    #[serde(default)]
    pub template:    Option<String>,
}

// These types are now defined in their own modules and re-exported above

/// Reserved: placeholder for future structured-logging configuration.
#[derive(Debug, Clone, Deserialize)]
pub struct LoggingConfig {}

/// Configuration for a single object-storage backend.
#[derive(Debug, Clone, Deserialize)]
pub struct StorageConfig {
    /// Storage backend identifier (e.g. `"s3"`, `"gcs"`, `"local"`).
    pub backend: String,
    /// Bucket or container name (required for cloud backends).
    #[serde(default)]
    pub bucket:  Option<String>,
    /// Local filesystem path (used by the `"local"` backend).
    #[serde(default)]
    pub path:    Option<String>,
}

/// Reserved: placeholder for future full-text search indexing configuration.
#[derive(Debug, Clone, Deserialize)]
pub struct SearchConfig {}

/// Reserved: placeholder for future advanced query-result caching configuration.
#[derive(Debug, Clone, Deserialize)]
pub struct CacheConfig {}

/// Reserved: placeholder for future background job-queue configuration.
#[derive(Debug, Clone, Deserialize)]
pub struct QueueConfig {}

/// Reserved: placeholder for future real-time subscription update configuration.
#[derive(Debug, Clone, Deserialize)]
pub struct RealtimeConfig {}

/// Reserved: placeholder for future custom HTTP endpoint configuration.
#[derive(Debug, Clone, Deserialize)]
pub struct CustomEndpointsConfig {}
