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
#[allow(deprecated)] // Reason: re-export deprecated alias for backwards compatibility
pub use pool_tuning::{PoolPressureMonitorConfig, PoolTuningConfig};
pub use rate_limiting::{BackpressureConfig, RateLimitRule, RateLimitingConfig};
pub use tracing::TracingConfig;

/// Configuration for durable usage counter persistence.
///
/// Add a `[usage]` section to `fraiseql.toml` (or `ServerConfig`) to enable:
///
/// ```toml
/// [usage]
/// flush_interval_secs = 60
/// ```
///
/// When absent (default), the [`NoopBackend`] is used and counters are
/// in-memory only (reset on process restart).
///
/// [`NoopBackend`]: crate::usage::aggregator::NoopBackend
#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
pub struct UsagePersistenceConfig {
    /// How often (in seconds) to flush in-memory counters to PostgreSQL.
    ///
    /// Defaults to `60` seconds.
    #[serde(default = "default_flush_interval_secs")]
    pub flush_interval_secs: u64,
}

const fn default_flush_interval_secs() -> u64 {
    60
}

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
    /// Maximum allowed request body size as a human-readable string (e.g. `"10MB"`).  Default:
    /// `"10MB"`.
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
    pub backend:          String,
    /// Bucket or container name (required for cloud backends).
    #[serde(default)]
    pub bucket:           Option<String>,
    /// Local filesystem path (used by the `"local"` backend).
    #[serde(default)]
    pub path:             Option<String>,
    /// Cloud region (e.g. `"eu-west-1"` for AWS, `"fr-par"` for Scaleway).
    #[serde(default)]
    pub region:           Option<String>,
    /// Custom endpoint URL (for S3-compatible providers, Azurite, and
    /// fake-gcs-server local-development emulators).
    #[serde(default)]
    pub endpoint:         Option<String>,
    /// GCP project ID (used by the `"gcs"` backend).
    #[serde(default)]
    pub project_id:       Option<String>,
    /// Azure storage account name (used by the `"azure"` backend).
    #[serde(default)]
    pub account_name:     Option<String>,
    /// Maximum upload size in bytes for this storage backend.
    ///
    /// Defaults to `104_857_600` (100 `MiB`). Uploads exceeding this size are
    /// rejected with HTTP 413 before touching the backend.
    #[serde(default = "default_max_upload_bytes")]
    pub max_upload_bytes: usize,
}

const fn default_max_upload_bytes() -> usize {
    100 * 1024 * 1024 // 100 MiB
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

// ---------------------------------------------------------------------------
// Builders
// ---------------------------------------------------------------------------

impl HttpServerConfig {
    /// Returns a builder for `HttpServerConfig`.
    #[must_use = "builder does nothing until .build() is called"]
    pub fn builder() -> HttpServerConfigBuilder {
        HttpServerConfigBuilder::default()
    }
}

/// Builder for [`HttpServerConfig`].
#[derive(Debug)]
pub struct HttpServerConfigBuilder {
    port:    u16,
    host:    String,
    workers: Option<usize>,
    tls:     Option<TlsConfig>,
    limits:  Option<ServerLimitsConfig>,
}

impl Default for HttpServerConfigBuilder {
    fn default() -> Self {
        Self {
            port:    default_port(),
            host:    default_host(),
            workers: None,
            tls:     None,
            limits:  None,
        }
    }
}

impl HttpServerConfigBuilder {
    /// Sets the TCP port to listen on.
    #[must_use = "builder method returns modified builder"]
    pub const fn port(mut self, port: u16) -> Self {
        self.port = port;
        self
    }

    /// Sets the network interface to bind.
    pub fn host(mut self, host: impl Into<String>) -> Self {
        self.host = host.into();
        self
    }

    /// Sets the number of async worker threads.
    #[must_use = "builder method returns modified builder"]
    pub const fn workers(mut self, workers: usize) -> Self {
        self.workers = Some(workers);
        self
    }

    /// Sets the TLS configuration.
    #[must_use = "builder method returns modified builder"]
    pub fn tls(mut self, tls: TlsConfig) -> Self {
        self.tls = Some(tls);
        self
    }

    /// Sets the per-request and concurrency limits.
    #[must_use = "builder method returns modified builder"]
    pub fn limits(mut self, limits: ServerLimitsConfig) -> Self {
        self.limits = Some(limits);
        self
    }

    /// Builds the [`HttpServerConfig`].
    #[must_use = "building a config that is not used has no effect"]
    pub fn build(self) -> HttpServerConfig {
        HttpServerConfig {
            port:    self.port,
            host:    self.host,
            workers: self.workers,
            tls:     self.tls,
            limits:  self.limits,
        }
    }
}

impl ServerLimitsConfig {
    /// Returns a builder for `ServerLimitsConfig`.
    #[must_use = "builder does nothing until .build() is called"]
    pub fn builder() -> ServerLimitsConfigBuilder {
        ServerLimitsConfigBuilder::default()
    }
}

/// Builder for [`ServerLimitsConfig`].
#[derive(Debug)]
pub struct ServerLimitsConfigBuilder {
    max_request_size:        String,
    request_timeout:         String,
    max_concurrent_requests: usize,
    max_queue_depth:         usize,
}

impl Default for ServerLimitsConfigBuilder {
    fn default() -> Self {
        Self {
            max_request_size:        default_max_request_size(),
            request_timeout:         default_request_timeout(),
            max_concurrent_requests: default_max_concurrent(),
            max_queue_depth:         default_max_queue_depth(),
        }
    }
}

impl ServerLimitsConfigBuilder {
    /// Sets the maximum request body size (e.g. `"10MB"`).
    pub fn max_request_size(mut self, max_request_size: impl Into<String>) -> Self {
        self.max_request_size = max_request_size.into();
        self
    }

    /// Sets the maximum request processing time (e.g. `"30s"`).
    pub fn request_timeout(mut self, request_timeout: impl Into<String>) -> Self {
        self.request_timeout = request_timeout.into();
        self
    }

    /// Sets the maximum number of concurrent requests.
    #[must_use = "builder method returns modified builder"]
    pub const fn max_concurrent_requests(mut self, max_concurrent_requests: usize) -> Self {
        self.max_concurrent_requests = max_concurrent_requests;
        self
    }

    /// Sets the maximum request queue depth.
    #[must_use = "builder method returns modified builder"]
    pub const fn max_queue_depth(mut self, max_queue_depth: usize) -> Self {
        self.max_queue_depth = max_queue_depth;
        self
    }

    /// Builds the [`ServerLimitsConfig`].
    #[must_use = "building a config that is not used has no effect"]
    pub fn build(self) -> ServerLimitsConfig {
        ServerLimitsConfig {
            max_request_size:        self.max_request_size,
            request_timeout:         self.request_timeout,
            max_concurrent_requests: self.max_concurrent_requests,
            max_queue_depth:         self.max_queue_depth,
        }
    }
}

impl DatabaseConfig {
    /// Returns a builder for `DatabaseConfig`.
    #[must_use = "builder does nothing until .build() is called"]
    pub fn builder() -> DatabaseConfigBuilder {
        DatabaseConfigBuilder::default()
    }
}

/// Builder for [`DatabaseConfig`].
#[derive(Debug, Default)]
pub struct DatabaseConfigBuilder {
    url_env:               Option<String>,
    pool_size:             u32,
    pool_timeout:          Option<String>,
    statement_timeout:     Option<String>,
    replicas:              Vec<ReplicaConfig>,
    health_check_interval: Option<String>,
}

impl DatabaseConfigBuilder {
    /// Sets the environment variable name that holds the database URL.
    pub fn url_env(mut self, url_env: impl Into<String>) -> Self {
        self.url_env = Some(url_env.into());
        self
    }

    /// Sets the maximum number of connections in the pool.
    #[must_use = "builder method returns modified builder"]
    pub const fn pool_size(mut self, pool_size: u32) -> Self {
        self.pool_size = pool_size;
        self
    }

    /// Sets how long to wait for a connection before returning an error.
    pub fn pool_timeout(mut self, pool_timeout: impl Into<String>) -> Self {
        self.pool_timeout = Some(pool_timeout.into());
        self
    }

    /// Sets the per-query statement timeout.
    pub fn statement_timeout(mut self, statement_timeout: impl Into<String>) -> Self {
        self.statement_timeout = Some(statement_timeout.into());
        self
    }

    /// Adds a read replica.
    #[must_use = "builder method returns modified builder"]
    pub fn replica(mut self, replica: ReplicaConfig) -> Self {
        self.replicas.push(replica);
        self
    }

    /// Sets the health-check ping interval.
    pub fn health_check_interval(mut self, health_check_interval: impl Into<String>) -> Self {
        self.health_check_interval = Some(health_check_interval.into());
        self
    }

    /// Builds the [`DatabaseConfig`].
    ///
    /// # Errors
    ///
    /// Returns an error string if `url_env` was not set.
    pub fn build(self) -> Result<DatabaseConfig, String> {
        let url_env =
            self.url_env.ok_or_else(|| "DatabaseConfig: url_env is required".to_string())?;
        Ok(DatabaseConfig {
            url_env,
            pool_size: if self.pool_size == 0 {
                default_pool_size()
            } else {
                self.pool_size
            },
            pool_timeout: self.pool_timeout,
            statement_timeout: self.statement_timeout,
            replicas: self.replicas,
            health_check_interval: self.health_check_interval,
        })
    }
}

impl LifecycleConfig {
    /// Returns a builder for `LifecycleConfig`.
    #[must_use = "builder does nothing until .build() is called"]
    pub fn builder() -> LifecycleConfigBuilder {
        LifecycleConfigBuilder::default()
    }
}

/// Builder for [`LifecycleConfig`].
#[derive(Debug)]
pub struct LifecycleConfigBuilder {
    shutdown_timeout: String,
    shutdown_delay:   String,
    health_path:      String,
    ready_path:       String,
}

impl Default for LifecycleConfigBuilder {
    fn default() -> Self {
        Self {
            shutdown_timeout: default_shutdown_timeout(),
            shutdown_delay:   default_shutdown_delay(),
            health_path:      default_health_path(),
            ready_path:       default_ready_path(),
        }
    }
}

impl LifecycleConfigBuilder {
    /// Sets the graceful-shutdown timeout (e.g. `"30s"`).
    pub fn shutdown_timeout(mut self, shutdown_timeout: impl Into<String>) -> Self {
        self.shutdown_timeout = shutdown_timeout.into();
        self
    }

    /// Sets the pre-shutdown delay for load balancer deregistration (e.g. `"5s"`).
    pub fn shutdown_delay(mut self, shutdown_delay: impl Into<String>) -> Self {
        self.shutdown_delay = shutdown_delay.into();
        self
    }

    /// Sets the health-check endpoint path.
    pub fn health_path(mut self, health_path: impl Into<String>) -> Self {
        self.health_path = health_path.into();
        self
    }

    /// Sets the readiness-check endpoint path.
    pub fn ready_path(mut self, ready_path: impl Into<String>) -> Self {
        self.ready_path = ready_path.into();
        self
    }

    /// Builds the [`LifecycleConfig`].
    #[must_use = "building a config that is not used has no effect"]
    pub fn build(self) -> LifecycleConfig {
        LifecycleConfig {
            shutdown_timeout: self.shutdown_timeout,
            shutdown_delay:   self.shutdown_delay,
            health_path:      self.health_path,
            ready_path:       self.ready_path,
        }
    }
}
