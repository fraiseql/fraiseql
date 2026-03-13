//! Server configuration.

pub mod tls;
pub mod observers;
pub(crate) mod defaults;

pub use tls::{PlaygroundTool, TlsServerConfig, DatabaseTlsConfig};
pub use observers::AdmissionConfig;
#[cfg(feature = "observers")]
pub use observers::ObserverConfig;

use std::{net::SocketAddr, path::PathBuf};

use fraiseql_core::security::OidcConfig;
use serde::{Deserialize, Serialize};

use crate::middleware::RateLimitConfig;

use defaults::{
    default_bind_addr, default_database_url, default_graphql_path, default_health_path,
    default_introspection_path, default_max_request_body_bytes, default_metrics_json_path,
    default_metrics_path, default_playground_path, default_pool_max_size, default_pool_min_size,
    default_pool_timeout, default_readiness_path, default_schema_path,
    default_shutdown_timeout_secs, default_subscription_path,
};

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

    /// Enable compression.
    #[serde(default = "defaults::default_true")]
    pub compression_enabled: bool,

    /// Enable request tracing.
    #[serde(default = "defaults::default_true")]
    pub tracing_enabled: bool,

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
    /// When enabled, serves a GraphQL IDE (GraphiQL or Apollo Sandbox)
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

    /// WebSocket endpoint path for GraphQL subscriptions.
    #[serde(default = "defaults::default_subscription_path")]
    pub subscription_path: String,

    /// Enable GraphQL subscriptions over WebSocket.
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
    /// per-database TLS settings (PostgreSQL, Redis, ClickHouse, etc.).
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
    /// strings are either a DoS attempt or a sign that the caller should use
    /// POST instead. Default: `100_000` (100 KiB).
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
            compression_enabled: true,
            tracing_enabled: true,
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
            auth: None,          // No auth by default
            tls: None,           // TLS disabled by default
            database_tls: None,  // Database TLS disabled by default
            require_json_content_type: true, // CSRF protection
            max_request_body_bytes: default_max_request_body_bytes(), // 1 MB
            rate_limiting: None, // Rate limiting uses defaults
            #[cfg(feature = "observers")]
            observers: None, // Observers disabled by default
            pool_tuning: None,  // Pool pressure monitoring disabled by default
            admission_control: None, // Admission control disabled by default
            shutdown_timeout_secs: default_shutdown_timeout_secs(),
            request_timeout_secs: None,
            max_get_query_bytes: defaults::default_max_get_query_bytes(),
        }
    }
}

impl ServerConfig {
    /// Load server configuration from a TOML file.
    ///
    /// # Errors
    ///
    /// Returns an error string if the file cannot be read or the TOML cannot be parsed.
    pub fn from_file(path: impl AsRef<std::path::Path>) -> Result<Self, String> {
        let content = std::fs::read_to_string(path.as_ref())
            .map_err(|e| format!("Cannot read config file: {e}"))?;
        toml::from_str(&content).map_err(|e| format!("Invalid TOML config: {e}"))
    }

    /// Check if running in production mode.
    ///
    /// Production mode is detected via `FRAISEQL_ENV` environment variable.
    /// - `production` or `prod` (or any value other than `development`/`dev`) → production mode
    /// - `development` or `dev` → development mode
    #[must_use]
    pub fn is_production_mode() -> bool {
        let env = std::env::var("FRAISEQL_ENV")
            .unwrap_or_else(|_| "production".to_string())
            .to_lowercase();
        env != "development" && env != "dev"
    }

    /// Validate configuration.
    ///
    /// # Errors
    ///
    /// Returns error if:
    /// - `metrics_enabled` is true but `metrics_token` is not set
    /// - `metrics_token` is set but too short (< 16 characters)
    /// - `auth` config is set but invalid (e.g., empty issuer)
    /// - `tls` is enabled but cert or key path is missing
    /// - TLS minimum version is invalid
    /// - In production mode: `playground_enabled` is true
    /// - In production mode: `cors_enabled` is true but `cors_origins` is empty
    pub fn validate(&self) -> Result<(), String> {
        if self.metrics_enabled {
            match &self.metrics_token {
                None => {
                    return Err("metrics_enabled is true but metrics_token is not set. \
                         Set FRAISEQL_METRICS_TOKEN or metrics_token in config."
                        .to_string());
                },
                Some(token) if token.len() < 16 => {
                    return Err(
                        "metrics_token must be at least 16 characters for security.".to_string()
                    );
                },
                Some(_) => {},
            }
        }

        // Admin API validation
        if self.admin_api_enabled {
            match &self.admin_token {
                None => {
                    return Err("admin_api_enabled is true but admin_token is not set. \
                         Set FRAISEQL_ADMIN_TOKEN or admin_token in config."
                        .to_string());
                },
                Some(token) if token.len() < 32 => {
                    return Err(
                        "admin_token must be at least 32 characters for security.".to_string()
                    );
                },
                Some(_) => {},
            }

            // Validate the optional read-only token when provided.
            if let Some(ref ro_token) = self.admin_readonly_token {
                if ro_token.len() < 32 {
                    return Err(
                        "admin_readonly_token must be at least 32 characters for security."
                            .to_string(),
                    );
                }
                if Some(ro_token) == self.admin_token.as_ref() {
                    return Err(
                        "admin_readonly_token must differ from admin_token.".to_string()
                    );
                }
            }
        }

        // Validate OIDC config if present
        if let Some(ref auth) = self.auth {
            auth.validate().map_err(|e| e.to_string())?;
        }

        // Validate TLS config if present and enabled
        if let Some(ref tls) = self.tls {
            if tls.enabled {
                if !tls.cert_path.exists() {
                    return Err(format!(
                        "TLS enabled but certificate file not found: {}",
                        tls.cert_path.display()
                    ));
                }
                if !tls.key_path.exists() {
                    return Err(format!(
                        "TLS enabled but key file not found: {}",
                        tls.key_path.display()
                    ));
                }

                // Validate TLS version
                if !["1.2", "1.3"].contains(&tls.min_version.as_str()) {
                    return Err("TLS min_version must be '1.2' or '1.3'".to_string());
                }

                // Validate mTLS config if required
                if tls.require_client_cert {
                    if let Some(ref ca_path) = tls.client_ca_path {
                        if !ca_path.exists() {
                            return Err(format!("Client CA file not found: {}", ca_path.display()));
                        }
                    } else {
                        return Err(
                            "require_client_cert is true but client_ca_path is not set".to_string()
                        );
                    }
                }
            }
        }

        // Pool invariants
        if self.pool_max_size == 0 {
            return Err("pool_max_size must be at least 1".to_string());
        }
        if self.pool_min_size > self.pool_max_size {
            return Err(format!(
                "pool_min_size ({}) must not exceed pool_max_size ({})",
                self.pool_min_size, self.pool_max_size
            ));
        }

        // Validate database TLS config if present
        if let Some(ref db_tls) = self.database_tls {
            // Validate PostgreSQL SSL mode
            if ![
                "disable",
                "allow",
                "prefer",
                "require",
                "verify-ca",
                "verify-full",
            ]
            .contains(&db_tls.postgres_ssl_mode.as_str())
            {
                return Err("Invalid postgres_ssl_mode. Must be one of: \
                     disable, allow, prefer, require, verify-ca, verify-full"
                    .to_string());
            }

            // Validate CA bundle path if provided
            if let Some(ref ca_path) = db_tls.ca_bundle_path {
                if !ca_path.exists() {
                    return Err(format!("CA bundle file not found: {}", ca_path.display()));
                }
            }
        }

        // Production safety validation
        if Self::is_production_mode() {
            // Playground should be disabled in production
            if self.playground_enabled {
                return Err("playground_enabled is true in production mode. \
                     Disable the playground or set FRAISEQL_ENV=development. \
                     The playground exposes sensitive schema information."
                    .to_string());
            }

            // CORS origins must be explicitly configured in production
            if self.cors_enabled && self.cors_origins.is_empty() {
                return Err("cors_enabled is true but cors_origins is empty in production mode. \
                     This allows requests from ANY origin, which is a security risk. \
                     Explicitly configure cors_origins with your allowed domains, \
                     or disable CORS and set FRAISEQL_ENV=development to bypass this check."
                    .to_string());
            }
        }

        Ok(())
    }

    /// Check if authentication is enabled.
    #[must_use]
    pub const fn auth_enabled(&self) -> bool {
        self.auth.is_some()
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)] // Reason: test code, panics acceptable
    #![allow(clippy::cast_precision_loss)] // Reason: test metrics reporting
    #![allow(clippy::cast_sign_loss)] // Reason: test data uses small positive integers
    #![allow(clippy::cast_possible_truncation)] // Reason: test data values are bounded
    #![allow(clippy::cast_possible_wrap)] // Reason: test data values are bounded
    #![allow(clippy::missing_panics_doc)] // Reason: test helpers
    #![allow(clippy::missing_errors_doc)] // Reason: test helpers
    #![allow(missing_docs)] // Reason: test code
    #![allow(clippy::items_after_statements)] // Reason: test helpers defined near use site

    use super::*;

    #[test]
    fn test_default_config() {
        let config = ServerConfig::default();
        assert_eq!(config.schema_path, PathBuf::from("schema.compiled.json"));
        assert_eq!(config.database_url, "postgresql://localhost/fraiseql");
        assert_eq!(config.graphql_path, "/graphql");
        assert_eq!(config.health_path, "/health");
        assert_eq!(config.metrics_path, "/metrics");
        assert_eq!(config.metrics_json_path, "/metrics/json");
        assert!(config.cors_enabled);
        assert!(config.compression_enabled);
    }

    #[test]
    fn test_default_config_metrics_disabled() {
        let config = ServerConfig::default();
        assert!(!config.metrics_enabled, "Metrics should be disabled by default for security");
        assert!(config.metrics_token.is_none());
    }

    #[test]
    fn test_config_with_custom_database_url() {
        let config = ServerConfig {
            database_url: "postgresql://user:pass@db.example.com/mydb".to_string(),
            ..ServerConfig::default()
        };
        assert_eq!(config.database_url, "postgresql://user:pass@db.example.com/mydb");
    }

    #[test]
    fn test_default_pool_config() {
        let config = ServerConfig::default();
        assert_eq!(config.pool_min_size, 5);
        assert_eq!(config.pool_max_size, 20);
        assert_eq!(config.pool_timeout_secs, 30);
    }

    #[test]
    fn test_config_with_custom_pool_size() {
        let config = ServerConfig {
            pool_min_size: 2,
            pool_max_size: 50,
            pool_timeout_secs: 60,
            ..ServerConfig::default()
        };
        assert_eq!(config.pool_min_size, 2);
        assert_eq!(config.pool_max_size, 50);
        assert_eq!(config.pool_timeout_secs, 60);
    }

    #[test]
    fn test_validate_metrics_disabled_ok() {
        let config = ServerConfig {
            cors_enabled: false,
            ..ServerConfig::default()
        };
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_validate_metrics_enabled_without_token_fails() {
        let config = ServerConfig {
            metrics_enabled: true,
            metrics_token: None,
            ..ServerConfig::default()
        };
        let result = config.validate();
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("metrics_token is not set"));
    }

    #[test]
    fn test_validate_metrics_enabled_with_short_token_fails() {
        let config = ServerConfig {
            metrics_enabled: true,
            metrics_token: Some("short".to_string()), // < 16 chars
            ..ServerConfig::default()
        };
        let result = config.validate();
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("at least 16 characters"));
    }

    #[test]
    fn test_validate_metrics_enabled_with_valid_token_ok() {
        let config = ServerConfig {
            metrics_enabled: true,
            metrics_token: Some("a-secure-token-that-is-long-enough".to_string()),
            cors_enabled: false,
            ..ServerConfig::default()
        };
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_default_subscription_config() {
        let config = ServerConfig::default();
        assert_eq!(config.subscription_path, "/ws");
        assert!(config.subscriptions_enabled);
    }

    #[test]
    fn test_subscription_config_with_custom_path() {
        let config = ServerConfig {
            subscription_path: "/subscriptions".to_string(),
            ..ServerConfig::default()
        };
        assert_eq!(config.subscription_path, "/subscriptions");
        assert!(config.subscriptions_enabled);
    }

    #[test]
    fn test_subscriptions_can_be_disabled() {
        let config = ServerConfig {
            subscriptions_enabled: false,
            ..ServerConfig::default()
        };
        assert!(!config.subscriptions_enabled);
        assert_eq!(config.subscription_path, "/ws");
    }

    #[test]
    fn test_subscription_path_serialization() {
        let config = ServerConfig::default();
        let json = serde_json::to_string(&config).expect(
            "ServerConfig derives Serialize with serializable fields; serialization is infallible",
        );
        let restored: ServerConfig = serde_json::from_str(&json).expect(
            "ServerConfig roundtrip: deserialization of just-serialized data is infallible",
        );

        assert_eq!(restored.subscription_path, config.subscription_path);
        assert_eq!(restored.subscriptions_enabled, config.subscriptions_enabled);
    }

    #[test]
    fn test_subscription_config_with_partial_toml() {
        let toml_str = r#"
            subscription_path = "/graphql-ws"
            subscriptions_enabled = false
        "#;

        let decoded: ServerConfig = toml::from_str(toml_str).expect(
            "TOML config parsing: valid TOML syntax with expected fields deserializes correctly",
        );
        assert_eq!(decoded.subscription_path, "/graphql-ws");
        assert!(!decoded.subscriptions_enabled);
    }

    #[test]
    fn test_tls_config_defaults() {
        let config = ServerConfig::default();
        assert!(config.tls.is_none());
        assert!(config.database_tls.is_none());
    }

    #[test]
    fn test_database_tls_config_defaults() {
        let db_tls = DatabaseTlsConfig {
            postgres_ssl_mode:   "prefer".to_string(),
            redis_ssl:           false,
            clickhouse_https:    false,
            elasticsearch_https: false,
            verify_certificates: true,
            ca_bundle_path:      None,
        };

        assert_eq!(db_tls.postgres_ssl_mode, "prefer");
        assert!(!db_tls.redis_ssl);
        assert!(!db_tls.clickhouse_https);
        assert!(!db_tls.elasticsearch_https);
        assert!(db_tls.verify_certificates);
    }

    #[test]
    fn test_tls_server_config_fields() {
        let tls = TlsServerConfig {
            enabled:             true,
            cert_path:           PathBuf::from("/etc/fraiseql/cert.pem"),
            key_path:            PathBuf::from("/etc/fraiseql/key.pem"),
            require_client_cert: false,
            client_ca_path:      None,
            min_version:         "1.3".to_string(),
        };

        assert!(tls.enabled);
        assert_eq!(tls.cert_path, PathBuf::from("/etc/fraiseql/cert.pem"));
        assert_eq!(tls.key_path, PathBuf::from("/etc/fraiseql/key.pem"));
        assert!(!tls.require_client_cert);
        assert_eq!(tls.min_version, "1.3");
    }

    #[test]
    fn test_validate_tls_enabled_without_cert() {
        let config = ServerConfig {
            tls: Some(TlsServerConfig {
                enabled:             true,
                cert_path:           PathBuf::from("/nonexistent/cert.pem"),
                key_path:            PathBuf::from("/etc/fraiseql/key.pem"),
                require_client_cert: false,
                client_ca_path:      None,
                min_version:         "1.2".to_string(),
            }),
            ..ServerConfig::default()
        };

        let result = config.validate();
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("certificate file not found"));
    }

    #[test]
    fn test_validate_tls_invalid_min_version() {
        // Create temp cert and key files that exist
        let cert_path = PathBuf::from("/tmp/test_cert.pem");
        let key_path = PathBuf::from("/tmp/test_key.pem");
        std::fs::write(&cert_path, "test").ok();
        std::fs::write(&key_path, "test").ok();

        let config = ServerConfig {
            tls: Some(TlsServerConfig {
                enabled: true,
                cert_path,
                key_path,
                require_client_cert: false,
                client_ca_path: None,
                min_version: "1.1".to_string(),
            }),
            ..ServerConfig::default()
        };

        let result = config.validate();
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("min_version must be"));
    }

    #[test]
    fn test_validate_database_tls_invalid_postgres_ssl_mode() {
        let config = ServerConfig {
            database_tls: Some(DatabaseTlsConfig {
                postgres_ssl_mode:   "invalid_mode".to_string(),
                redis_ssl:           false,
                clickhouse_https:    false,
                elasticsearch_https: false,
                verify_certificates: true,
                ca_bundle_path:      None,
            }),
            ..ServerConfig::default()
        };

        let result = config.validate();
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Invalid postgres_ssl_mode"));
    }

    #[test]
    fn test_validate_tls_requires_client_ca() {
        // Create temp cert and key files that exist
        let cert_path = PathBuf::from("/tmp/test_cert2.pem");
        let key_path = PathBuf::from("/tmp/test_key2.pem");
        std::fs::write(&cert_path, "test").ok();
        std::fs::write(&key_path, "test").ok();

        let config = ServerConfig {
            tls: Some(TlsServerConfig {
                enabled: true,
                cert_path,
                key_path,
                require_client_cert: true,
                client_ca_path: None,
                min_version: "1.3".to_string(),
            }),
            ..ServerConfig::default()
        };

        let result = config.validate();
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("client_ca_path is not set"));
    }

    #[test]
    fn test_database_tls_serialization() {
        let db_tls = DatabaseTlsConfig {
            postgres_ssl_mode:   "require".to_string(),
            redis_ssl:           true,
            clickhouse_https:    true,
            elasticsearch_https: true,
            verify_certificates: true,
            ca_bundle_path:      Some(PathBuf::from("/etc/ssl/certs/ca-bundle.crt")),
        };

        let json = serde_json::to_string(&db_tls)
            .expect("DatabaseTlsConfig derives Serialize with serializable fields; serialization is infallible");
        let restored: DatabaseTlsConfig = serde_json::from_str(&json).expect(
            "DatabaseTlsConfig roundtrip: deserialization of just-serialized data is infallible",
        );

        assert_eq!(restored.postgres_ssl_mode, db_tls.postgres_ssl_mode);
        assert_eq!(restored.redis_ssl, db_tls.redis_ssl);
        assert_eq!(restored.clickhouse_https, db_tls.clickhouse_https);
        assert_eq!(restored.elasticsearch_https, db_tls.elasticsearch_https);
        assert_eq!(restored.ca_bundle_path, db_tls.ca_bundle_path);
    }

    #[test]
    fn test_admin_api_disabled_by_default() {
        let config = ServerConfig::default();
        assert!(
            !config.admin_api_enabled,
            "Admin API should be disabled by default for security"
        );
        assert!(config.admin_token.is_none());
    }

    #[test]
    fn test_validate_admin_api_enabled_without_token_fails() {
        let config = ServerConfig {
            admin_api_enabled: true,
            admin_token: None,
            ..ServerConfig::default()
        };
        let result = config.validate();
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("admin_token is not set"));
    }

    #[test]
    fn test_validate_admin_api_enabled_with_short_token_fails() {
        let config = ServerConfig {
            admin_api_enabled: true,
            admin_token: Some("short".to_string()), // < 32 chars
            ..ServerConfig::default()
        };
        let result = config.validate();
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("at least 32 characters"));
    }

    #[test]
    fn test_validate_admin_api_enabled_with_valid_token_ok() {
        let config = ServerConfig {
            admin_api_enabled: true,
            admin_token: Some("a-very-secure-admin-token-that-is-long-enough".to_string()),
            cors_enabled: false,
            ..ServerConfig::default()
        };
        assert!(config.validate().is_ok());
    }

    // --- admin_readonly_token validation tests (S10-1) ---

    #[test]
    fn test_validate_admin_readonly_token_short_fails() {
        let config = ServerConfig {
            admin_api_enabled: true,
            admin_token: Some("a-very-secure-admin-token-that-is-long-enough".to_string()),
            admin_readonly_token: Some("short".to_string()),
            cors_enabled: false,
            ..ServerConfig::default()
        };
        let err = config.validate().unwrap_err();
        assert!(
            err.contains("admin_readonly_token must be at least 32"),
            "expected length error, got: {err}"
        );
    }

    #[test]
    fn test_validate_admin_readonly_token_same_as_admin_token_fails() {
        let token = "a-very-secure-admin-token-that-is-long-enough".to_string();
        let config = ServerConfig {
            admin_api_enabled: true,
            admin_token: Some(token.clone()),
            admin_readonly_token: Some(token),
            cors_enabled: false,
            ..ServerConfig::default()
        };
        let err = config.validate().unwrap_err();
        assert!(
            err.contains("must differ from admin_token"),
            "expected differ error, got: {err}"
        );
    }

    #[test]
    fn test_validate_admin_readonly_token_valid_passes() {
        let config = ServerConfig {
            admin_api_enabled: true,
            admin_token: Some("admin-write-token-that-is-long-enough-1234".to_string()),
            admin_readonly_token: Some("admin-readonly-token-that-is-long-enough-5678".to_string()),
            cors_enabled: false,
            ..ServerConfig::default()
        };
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_validate_admin_readonly_token_without_admin_enabled_is_ignored() {
        // admin_readonly_token with admin_api_enabled=false — validation skipped entirely.
        let config = ServerConfig {
            admin_api_enabled: false,
            admin_token: None,
            admin_readonly_token: Some("short".to_string()), // would fail if admin_api_enabled=true
            cors_enabled: false,
            ..ServerConfig::default()
        };
        assert!(config.validate().is_ok());
    }
}
