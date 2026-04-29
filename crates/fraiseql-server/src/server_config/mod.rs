//! Server configuration (`*Config` types).
//!
//! These are developer-facing configuration types loaded from `fraiseql.toml`,
//! environment variables, or CLI flags. They are mutable between deployments.
//!
//! For the distinction between `*Config` (developer-facing, mutable) and
//! `*Settings` (compiled into `schema.compiled.json`, immutable at runtime),
//! see `docs/architecture/config-vs-settings.md`.

pub(crate) mod defaults;
pub mod hs256;
mod methods;
pub mod observers;
pub mod tls;

#[cfg(test)]
mod tests;

use std::{net::SocketAddr, path::PathBuf};

use defaults::{
    default_bind_addr, default_database_url, default_graphql_path, default_health_path,
    default_introspection_path, default_max_header_bytes, default_max_header_count,
    default_max_request_body_bytes, default_metrics_json_path, default_metrics_path,
    default_playground_path, default_pool_max_size, default_pool_min_size, default_pool_timeout,
    default_readiness_path, default_schema_path, default_shutdown_timeout_secs,
    default_subscription_path,
};
use fraiseql_core::security::OidcConfig;
pub use hs256::Hs256Config;
pub use observers::AdmissionConfig;
#[cfg(feature = "observers")]
pub use observers::{ObserverConfig, ObserverPoolConfig};
use serde::{Deserialize, Serialize};
pub use tls::{DatabaseTlsConfig, PlaygroundTool, TlsServerConfig};

use crate::middleware::RateLimitConfig;

/// Server configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerConfig {
    /// Path to compiled schema JSON file.
    #[serde(default = "defaults::default_schema_path")]
    pub schema_path: PathBuf,

    /// Database connection URL (PostgreSQL, MySQL, SQLite, SQL Server).
    #[serde(default = "defaults::default_database_url")]
    pub database_url: String,

    /// Server bind address.
    #[serde(default = "defaults::default_bind_addr")]
    pub bind_addr: SocketAddr,

    /// Arrow Flight gRPC bind address (requires `arrow` feature).
    ///
    /// Defaults to `0.0.0.0:50051`. Override with `FRAISEQL_FLIGHT_BIND_ADDR`
    /// environment variable or this field in the config file.
    #[cfg(feature = "arrow")]
    #[serde(default = "defaults::default_flight_bind_addr")]
    pub flight_bind_addr: SocketAddr,

    /// Enable CORS.
    #[serde(default = "defaults::default_true")]
    pub cors_enabled: bool,

    /// CORS allowed origins (if empty, allows all).
    #[serde(default)]
    pub cors_origins: Vec<String>,

    /// Enable framework-level response compression.
    ///
    /// Defaults to `false`. In production FraiseQL is typically deployed
    /// behind a reverse proxy (Nginx, Caddy, cloud load balancer) that
    /// handles compression more efficiently (brotli, shared across upstreams,
    /// cacheable). Enable this only for single-binary / no-proxy deployments.
    #[serde(default = "defaults::default_false")]
    pub compression_enabled: bool,

    /// Enable request tracing.
    #[serde(default = "defaults::default_true")]
    pub tracing_enabled: bool,

    /// OTLP exporter endpoint for distributed tracing.
    ///
    /// When set (e.g. `"http://otel-collector:4317"`), the server initializes an
    /// `OpenTelemetry` OTLP exporter. When `None`, the `OTEL_EXPORTER_OTLP_ENDPOINT`
    /// environment variable is checked as a fallback. If neither is set, no OTLP
    /// export occurs (zero overhead).
    #[serde(default)]
    pub otlp_endpoint: Option<String>,

    /// OTLP exporter timeout in seconds (default: 10).
    #[serde(default = "defaults::default_otlp_timeout_secs")]
    pub otlp_export_timeout_secs: u64,

    /// Service name for distributed tracing (default: `"fraiseql"`).
    #[serde(default = "defaults::default_service_name")]
    pub tracing_service_name: String,

    /// Enable APQ (Automatic Persisted Queries).
    #[serde(default = "defaults::default_true")]
    pub apq_enabled: bool,

    /// Enable query caching.
    #[serde(default = "defaults::default_true")]
    pub cache_enabled: bool,

    /// GraphQL endpoint path.
    #[serde(default = "defaults::default_graphql_path")]
    pub graphql_path: String,

    /// Health check endpoint path (liveness probe).
    ///
    /// Returns 200 as long as the process is alive, 503 if the database is down.
    #[serde(default = "defaults::default_health_path")]
    pub health_path: String,

    /// Readiness probe endpoint path.
    ///
    /// Returns 200 when the server is ready to serve traffic (database reachable),
    /// 503 otherwise. Kubernetes `readinessProbe` should point here.
    #[serde(default = "defaults::default_readiness_path")]
    pub readiness_path: String,

    /// Introspection endpoint path.
    #[serde(default = "defaults::default_introspection_path")]
    pub introspection_path: String,

    /// Metrics endpoint path (Prometheus format).
    #[serde(default = "defaults::default_metrics_path")]
    pub metrics_path: String,

    /// Metrics JSON endpoint path.
    #[serde(default = "defaults::default_metrics_json_path")]
    pub metrics_json_path: String,

    /// Playground (GraphQL IDE) endpoint path.
    #[serde(default = "defaults::default_playground_path")]
    pub playground_path: String,

    /// Enable GraphQL playground/IDE (default: false for production safety).
    ///
    /// When enabled, serves a GraphQL IDE (`GraphiQL` or Apollo Sandbox)
    /// at the configured `playground_path`.
    ///
    /// **Security**: Disabled by default for production safety. Set to true for development
    /// environments only. The playground exposes schema information and can be a
    /// reconnaissance vector for attackers.
    #[serde(default)]
    pub playground_enabled: bool,

    /// Which GraphQL IDE to use.
    ///
    /// - `graphiql`: The classic GraphQL IDE (default)
    /// - `apollo-sandbox`: Apollo's embeddable sandbox
    #[serde(default)]
    pub playground_tool: PlaygroundTool,

    /// `WebSocket` endpoint path for GraphQL subscriptions.
    #[serde(default = "defaults::default_subscription_path")]
    pub subscription_path: String,

    /// Enable GraphQL subscriptions over `WebSocket`.
    ///
    /// When enabled, provides graphql-ws (graphql-transport-ws) protocol
    /// support for real-time subscription events.
    #[serde(default = "defaults::default_true")]
    pub subscriptions_enabled: bool,

    /// Enable metrics endpoints.
    ///
    /// **Security**: Disabled by default for production safety.
    /// When enabled, requires `metrics_token` to be set for authentication.
    #[serde(default)]
    pub metrics_enabled: bool,

    /// Bearer token for metrics endpoint authentication.
    ///
    /// Required when `metrics_enabled` is true. Requests must include:
    /// `Authorization: Bearer <token>`
    ///
    /// **Security**: Use a strong, random token (e.g., 32+ characters).
    #[serde(default)]
    pub metrics_token: Option<String>,

    /// Enable admin API endpoints (default: false for production safety).
    ///
    /// **Security**: Disabled by default. When enabled, requires `admin_token` to be set.
    /// Admin endpoints allow schema reloading, cache management, and config inspection.
    #[serde(default)]
    pub admin_api_enabled: bool,

    /// Bearer token for admin API authentication.
    ///
    /// Required when `admin_api_enabled` is true. Requests must include:
    /// `Authorization: Bearer <token>`
    ///
    /// **Security**: Use a strong, random token (minimum 32 characters).
    /// This token grants access to **destructive** admin operations:
    /// `reload-schema`, `cache/clear`.
    ///
    /// If `admin_readonly_token` is set, this token is restricted to write
    /// operations only. If `admin_readonly_token` is not set, this token
    /// also grants access to read-only endpoints (backwards-compatible).
    #[serde(default)]
    pub admin_token: Option<String>,

    /// Optional separate bearer token for read-only admin operations.
    ///
    /// When set, restricts `admin_token` to destructive operations only
    /// (`reload-schema`, `cache/clear`) and uses this token for read-only
    /// endpoints (`config`, `cache/stats`, `explain`, `grafana-dashboard`).
    ///
    /// Operators and monitoring tools can use this token without gaining
    /// the ability to modify server state or reload the schema.
    ///
    /// **Security**: Must be different from `admin_token` and at least 32
    /// characters. Requires `admin_api_enabled = true` and `admin_token` set.
    #[serde(default)]
    pub admin_readonly_token: Option<String>,

    /// Enable introspection endpoint (default: false for production safety).
    ///
    /// **Security**: Disabled by default. When enabled, the introspection endpoint
    /// exposes the complete GraphQL schema structure. Combined with `introspection_require_auth`,
    /// you can optionally protect it with OIDC authentication.
    #[serde(default)]
    pub introspection_enabled: bool,

    /// Require authentication for introspection endpoint (default: true).
    ///
    /// When true and OIDC is configured, introspection requires same auth as GraphQL endpoint.
    /// When false, introspection is publicly accessible (use only in development).
    #[serde(default = "defaults::default_true")]
    pub introspection_require_auth: bool,

    /// Require authentication for design audit API endpoints (default: true).
    ///
    /// Design audit endpoints expose system architecture and optimization opportunities.
    /// When true and OIDC is configured, design endpoints require same auth as GraphQL endpoint.
    /// When false, design endpoints are publicly accessible (use only in development).
    #[serde(default = "defaults::default_true")]
    pub design_api_require_auth: bool,

    /// Database connection pool minimum size.
    #[serde(default = "defaults::default_pool_min_size")]
    pub pool_min_size: usize,

    /// Database connection pool maximum size.
    #[serde(default = "defaults::default_pool_max_size")]
    pub pool_max_size: usize,

    /// Database connection pool timeout in seconds.
    #[serde(default = "defaults::default_pool_timeout")]
    pub pool_timeout_secs: u64,

    /// OIDC authentication configuration (optional).
    ///
    /// When set, enables JWT authentication using OIDC discovery.
    /// Supports Auth0, Keycloak, Okta, Cognito, Azure AD, and any
    /// OIDC-compliant provider.
    ///
    /// # Example (TOML)
    ///
    /// ```toml
    /// [auth]
    /// issuer = "https://your-tenant.auth0.com/"
    /// audience = "your-api-identifier"
    /// ```
    #[serde(default)]
    pub auth: Option<OidcConfig>,

    /// HS256 symmetric-key authentication (optional).
    ///
    /// Alternative to `auth` (OIDC) for integration testing and internal
    /// service-to-service scenarios. Mutually exclusive with `auth`.
    ///
    /// Validation is fully local — no discovery endpoint, no JWKS fetch.
    /// Not recommended for public-facing production.
    ///
    /// # Example (TOML)
    ///
    /// ```toml
    /// [auth_hs256]
    /// secret_env = "FRAISEQL_HS256_SECRET"
    /// issuer = "my-test-suite"
    /// audience = "my-api"
    /// ```
    #[serde(default)]
    pub auth_hs256: Option<Hs256Config>,

    /// Named object-storage backend configurations, keyed by bucket name.
    ///
    /// When non-empty, the server mounts storage API routes at `/storage/v1/`.
    ///
    /// # Example (TOML)
    ///
    /// ```toml
    /// [storage.avatars]
    /// backend = "local"
    /// path = "/var/data/avatars"
    /// ```
    #[serde(default)]
    pub storage: std::collections::HashMap<String, fraiseql_storage::StorageConfig>,

    /// TLS/SSL configuration for HTTPS and encrypted connections.
    ///
    /// When set, enables TLS enforcement for HTTP/gRPC endpoints and
    /// optionally requires mutual TLS (mTLS) for client certificates.
    ///
    /// # Example (TOML)
    ///
    /// ```toml
    /// [tls]
    /// enabled = true
    /// cert_path = "/etc/fraiseql/cert.pem"
    /// key_path = "/etc/fraiseql/key.pem"
    /// require_client_cert = false
    /// min_version = "1.2"  # "1.2" or "1.3"
    /// ```
    #[serde(default)]
    pub tls: Option<TlsServerConfig>,

    /// Database TLS configuration.
    ///
    /// Enables TLS for database connections and configures
    /// per-database TLS settings (PostgreSQL, Redis, `ClickHouse`, etc.).
    ///
    /// # Example (TOML)
    ///
    /// ```toml
    /// [database_tls]
    /// postgres_ssl_mode = "require"  # disable, allow, prefer, require, verify-ca, verify-full
    /// redis_ssl = true               # Use rediss:// protocol
    /// clickhouse_https = true         # Use HTTPS
    /// elasticsearch_https = true      # Use HTTPS
    /// verify_certificates = true      # Verify server certificates
    /// ```
    #[serde(default)]
    pub database_tls: Option<DatabaseTlsConfig>,

    /// Require `Content-Type: application/json` on POST requests (default: true).
    ///
    /// CSRF protection: rejects POST requests with non-JSON Content-Type
    /// (e.g. `text/plain`, `application/x-www-form-urlencoded`) with 415.
    #[serde(default = "defaults::default_true")]
    pub require_json_content_type: bool,

    /// Maximum request body size in bytes (default: 1 MB).
    ///
    /// Requests exceeding this limit receive 413 Payload Too Large.
    /// Set to 0 to use axum's default (no limit).
    #[serde(default = "defaults::default_max_request_body_bytes")]
    pub max_request_body_bytes: usize,

    /// Maximum number of HTTP headers per request (default: 100).
    ///
    /// Requests with more headers than this limit receive 431 Request Header Fields Too Large.
    /// Prevents header-flooding `DoS` attacks that exhaust memory.
    #[serde(default = "defaults::default_max_header_count")]
    pub max_header_count: usize,

    /// Maximum total size of all HTTP headers in bytes (default: 32 `KiB`).
    ///
    /// Requests whose combined header name+value bytes exceed this limit receive
    /// 431 Request Header Fields Too Large. Prevents memory exhaustion from
    /// oversized header values.
    #[serde(default = "defaults::default_max_header_bytes")]
    pub max_header_bytes: usize,

    /// Per-request processing timeout in seconds (default: `None` — no timeout).
    ///
    /// When set, each HTTP request must complete within this many seconds or
    /// the server returns **408 Request Timeout**.  This is a defence-in-depth
    /// measure against slow or runaway database queries.
    ///
    /// **Recommendation**: set to `60` for production deployments.
    ///
    /// # Example (TOML)
    ///
    /// ```toml
    /// request_timeout_secs = 60
    /// ```
    #[serde(default)]
    pub request_timeout_secs: Option<u64>,

    /// Maximum byte length for a query string delivered via HTTP GET.
    ///
    /// GET queries are URL-encoded and passed as a query parameter. Very long
    /// strings are either a `DoS` attempt or a sign that the caller should use
    /// POST instead. Default: `100_000` (100 `KiB`).
    ///
    /// # Example (TOML)
    ///
    /// ```toml
    /// max_get_query_bytes = 50000
    /// ```
    #[serde(default = "defaults::default_max_get_query_bytes")]
    pub max_get_query_bytes: usize,

    /// Rate limiting configuration for GraphQL requests.
    ///
    /// When configured, enables per-IP and per-user rate limiting with token bucket algorithm.
    /// Defaults to enabled with sensible per-IP limits for security-by-default.
    ///
    /// # Example (TOML)
    ///
    /// ```toml
    /// [rate_limiting]
    /// enabled = true
    /// rps_per_ip = 100      # 100 requests/second per IP
    /// rps_per_user = 1000   # 1000 requests/second per authenticated user
    /// burst_size = 500      # Allow bursts up to 500 requests
    /// ```
    #[serde(default)]
    pub rate_limiting: Option<RateLimitConfig>,

    /// Observer runtime configuration (optional, requires `observers` feature).
    #[cfg(feature = "observers")]
    #[serde(default)]
    pub observers: Option<ObserverConfig>,

    /// Connection pool pressure monitoring configuration.
    ///
    /// When `enabled = true`, the server spawns a background task that monitors
    /// pool metrics and emits scaling recommendations via Prometheus metrics and
    /// log lines. **The pool is not resized at runtime** — act on
    /// `fraiseql_pool_tuning_*` events by adjusting `max_connections` and restarting.
    ///
    /// # Example (TOML)
    ///
    /// ```toml
    /// [pool_tuning]
    /// enabled = true
    /// min_pool_size = 5
    /// max_pool_size = 50
    /// tuning_interval_ms = 30000
    /// ```
    #[serde(default)]
    pub pool_tuning: Option<crate::config::pool_tuning::PoolPressureMonitorConfig>,

    /// Admission control configuration.
    ///
    /// When set, enforces a maximum number of concurrent in-flight requests and
    /// a maximum queue depth.  Requests that exceed either limit receive
    /// `503 Service Unavailable` immediately instead of stalling under load.
    ///
    /// # Example (TOML)
    ///
    /// ```toml
    /// [admission_control]
    /// max_concurrent = 500
    /// max_queue_depth = 1000
    /// ```
    #[serde(default)]
    pub admission_control: Option<AdmissionConfig>,

    /// Security contact email for `/.well-known/security.txt` (RFC 9116).
    ///
    /// When set, the server exposes a `/.well-known/security.txt` endpoint
    /// with this email address as the security contact. This helps security
    /// researchers report vulnerabilities responsibly.
    ///
    /// # Example (TOML)
    ///
    /// ```toml
    /// security_contact = "security@example.com"
    /// ```
    #[serde(default)]
    pub security_contact: Option<String>,

    /// Query validation overrides (depth and complexity limits).
    ///
    /// When present, these values take precedence over the limits baked into
    /// the compiled schema, allowing operators to tune validation without
    /// recompiling.
    ///
    /// # Example (TOML)
    ///
    /// ```toml
    /// [validation]
    /// max_query_depth = 15
    /// max_query_complexity = 200
    /// ```
    #[serde(default)]
    pub validation: Option<fraiseql_core::schema::ValidationConfig>,

    /// Graceful shutdown drain timeout in seconds (default: 30).
    ///
    /// After a SIGTERM or Ctrl+C signal, the server stops accepting new connections and
    /// waits for in-flight requests and background runtimes (observers) to finish.
    /// If the drain takes longer than this value, the process logs a warning and exits
    /// immediately instead of hanging indefinitely.
    ///
    /// Set this to match `terminationGracePeriodSeconds` in your Kubernetes pod spec
    /// minus a small buffer (e.g., 25s when `terminationGracePeriodSeconds = 30`).
    ///
    /// Override with `FRAISEQL_SHUTDOWN_TIMEOUT_SECS`.
    #[serde(default = "defaults::default_shutdown_timeout_secs")]
    pub shutdown_timeout_secs: u64,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            schema_path: default_schema_path(),
            database_url: default_database_url(),
            bind_addr: default_bind_addr(),
            #[cfg(feature = "arrow")]
            flight_bind_addr: defaults::default_flight_bind_addr(),
            cors_enabled: true,
            cors_origins: Vec::new(),
            compression_enabled: false,
            tracing_enabled: true,
            otlp_endpoint: None,
            otlp_export_timeout_secs: defaults::default_otlp_timeout_secs(),
            tracing_service_name: defaults::default_service_name(),
            apq_enabled: true,
            cache_enabled: true,
            graphql_path: default_graphql_path(),
            health_path: default_health_path(),
            readiness_path: default_readiness_path(),
            introspection_path: default_introspection_path(),
            metrics_path: default_metrics_path(),
            metrics_json_path: default_metrics_json_path(),
            playground_path: default_playground_path(),
            playground_enabled: false, // Disabled by default for security
            playground_tool: PlaygroundTool::default(),
            subscription_path: default_subscription_path(),
            subscriptions_enabled: true,
            metrics_enabled: false, // Disabled by default for security
            metrics_token: None,
            admin_api_enabled: false, // Disabled by default for security
            admin_token: None,
            admin_readonly_token: None,
            introspection_enabled: false, // Disabled by default for security
            introspection_require_auth: true, // Require auth when enabled
            design_api_require_auth: true, // Require auth for design endpoints
            pool_min_size: default_pool_min_size(),
            pool_max_size: default_pool_max_size(),
            pool_timeout_secs: default_pool_timeout(),
            auth: None,       // No auth by default
            auth_hs256: None, // No HS256 auth by default
            tls: None,        // TLS disabled by default
            database_tls: None, /* Database TLS disabled
                               * by default */
            require_json_content_type: true, // CSRF protection
            max_request_body_bytes: default_max_request_body_bytes(), // 1 MB
            max_header_count: default_max_header_count(), // 100 headers
            max_header_bytes: default_max_header_bytes(), // 32 KiB
            rate_limiting: None,             // Rate limiting uses defaults
            #[cfg(feature = "observers")]
            observers: None, // Observers disabled by default
            pool_tuning: None,               // Pool pressure monitoring disabled by default
            admission_control: None,         // Admission control disabled by default
            security_contact: None,          // No security.txt by default
            validation: None,                // Use compiled schema defaults
            shutdown_timeout_secs: default_shutdown_timeout_secs(),
            request_timeout_secs: None,
            max_get_query_bytes: defaults::default_max_get_query_bytes(),
            storage: std::collections::HashMap::new(),
        }
    }
}
