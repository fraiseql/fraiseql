//! Server configuration.

use std::{net::SocketAddr, path::PathBuf};

use fraiseql_core::security::OidcConfig;
use serde::{Deserialize, Serialize};

/// GraphQL IDE/playground tool to use.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum PlaygroundTool {
    /// GraphiQL - the classic GraphQL IDE.
    GraphiQL,
    /// Apollo Sandbox - Apollo's embeddable GraphQL IDE (default).
    ///
    /// Apollo Sandbox offers a better UX with features like:
    /// - Query collections and history
    /// - Schema documentation explorer
    /// - Variables and headers panels
    /// - Operation tracing
    #[default]
    ApolloSandbox,
}

/// TLS server configuration for HTTPS and secure connections.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TlsServerConfig {
    /// Enable TLS for HTTP/gRPC endpoints.
    pub enabled: bool,

    /// Path to TLS certificate file (PEM format).
    pub cert_path: PathBuf,

    /// Path to TLS private key file (PEM format).
    pub key_path: PathBuf,

    /// Require client certificate (mTLS) for all connections.
    #[serde(default)]
    pub require_client_cert: bool,

    /// Path to CA certificate for validating client certificates (for mTLS).
    #[serde(default)]
    pub client_ca_path: Option<PathBuf>,

    /// Minimum TLS version ("1.2" or "1.3", default: "1.2").
    #[serde(default = "default_tls_min_version")]
    pub min_version: String,
}

/// Database TLS configuration for encrypted database connections.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatabaseTlsConfig {
    /// PostgreSQL SSL mode: disable, allow, prefer, require, verify-ca, verify-full.
    #[serde(default = "default_postgres_ssl_mode")]
    pub postgres_ssl_mode: String,

    /// Enable TLS for Redis connections (use rediss:// protocol).
    #[serde(default = "default_redis_ssl")]
    pub redis_ssl: bool,

    /// Enable HTTPS for ClickHouse connections.
    #[serde(default = "default_clickhouse_https")]
    pub clickhouse_https: bool,

    /// Enable HTTPS for Elasticsearch connections.
    #[serde(default = "default_elasticsearch_https")]
    pub elasticsearch_https: bool,

    /// Verify server certificates for HTTPS connections.
    #[serde(default = "default_verify_certs")]
    pub verify_certificates: bool,

    /// Path to CA certificate bundle for verifying server certificates.
    #[serde(default)]
    pub ca_bundle_path: Option<PathBuf>,
}

/// Rate limiting configuration for GraphQL requests.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RateLimitingConfig {
    /// Enable rate limiting (default: true for security).
    #[serde(default = "default_rate_limiting_enabled")]
    pub enabled: bool,

    /// Requests per second per IP address.
    #[serde(default = "default_rate_limit_rps_per_ip")]
    pub rps_per_ip: u32,

    /// Requests per second per authenticated user.
    #[serde(default = "default_rate_limit_rps_per_user")]
    pub rps_per_user: u32,

    /// Burst capacity (maximum accumulated tokens).
    #[serde(default = "default_rate_limit_burst_size")]
    pub burst_size: u32,

    /// Cleanup interval for stale entries (seconds).
    #[serde(default = "default_rate_limit_cleanup_interval")]
    pub cleanup_interval_secs: u64,
}

/// Server configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerConfig {
    /// Path to compiled schema JSON file.
    #[serde(default = "default_schema_path")]
    pub schema_path: PathBuf,

    /// Database connection URL (PostgreSQL, MySQL, SQLite, SQL Server).
    #[serde(default = "default_database_url")]
    pub database_url: String,

    /// Server bind address.
    #[serde(default = "default_bind_addr")]
    pub bind_addr: SocketAddr,

    /// Enable CORS.
    #[serde(default = "default_true")]
    pub cors_enabled: bool,

    /// CORS allowed origins (if empty, allows all).
    #[serde(default)]
    pub cors_origins: Vec<String>,

    /// Enable compression.
    #[serde(default = "default_true")]
    pub compression_enabled: bool,

    /// Enable request tracing.
    #[serde(default = "default_true")]
    pub tracing_enabled: bool,

    /// Enable APQ (Automatic Persisted Queries).
    #[serde(default = "default_true")]
    pub apq_enabled: bool,

    /// Enable query caching.
    #[serde(default = "default_true")]
    pub cache_enabled: bool,

    /// GraphQL endpoint path.
    #[serde(default = "default_graphql_path")]
    pub graphql_path: String,

    /// Health check endpoint path.
    #[serde(default = "default_health_path")]
    pub health_path: String,

    /// Introspection endpoint path.
    #[serde(default = "default_introspection_path")]
    pub introspection_path: String,

    /// Metrics endpoint path (Prometheus format).
    #[serde(default = "default_metrics_path")]
    pub metrics_path: String,

    /// Metrics JSON endpoint path.
    #[serde(default = "default_metrics_json_path")]
    pub metrics_json_path: String,

    /// Playground (GraphQL IDE) endpoint path.
    #[serde(default = "default_playground_path")]
    pub playground_path: String,

    /// Enable GraphQL playground/IDE (default: false for production safety).
    ///
    /// When enabled, serves a GraphQL IDE (GraphiQL or Apollo Sandbox)
    /// at the configured `playground_path`.
    ///
    /// **Security**: Disabled by default for production safety. Set to true for development environments only.
    /// The playground exposes schema information and can be a reconnaissance vector for attackers.
    #[serde(default)]
    pub playground_enabled: bool,

    /// Which GraphQL IDE to use.
    ///
    /// - `graphiql`: The classic GraphQL IDE (default)
    /// - `apollo-sandbox`: Apollo's embeddable sandbox
    #[serde(default)]
    pub playground_tool: PlaygroundTool,

    /// WebSocket endpoint path for GraphQL subscriptions.
    #[serde(default = "default_subscription_path")]
    pub subscription_path: String,

    /// Enable GraphQL subscriptions over WebSocket.
    ///
    /// When enabled, provides graphql-ws (graphql-transport-ws) protocol
    /// support for real-time subscription events.
    #[serde(default = "default_true")]
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
    /// This token grants access to sensitive operations like schema reloading.
    #[serde(default)]
    pub admin_token: Option<String>,

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
    #[serde(default = "default_true")]
    pub introspection_require_auth: bool,

    /// Require authentication for design audit API endpoints (default: true).
    ///
    /// Design audit endpoints expose system architecture and optimization opportunities.
    /// When true and OIDC is configured, design endpoints require same auth as GraphQL endpoint.
    /// When false, design endpoints are publicly accessible (use only in development).
    #[serde(default = "default_true")]
    pub design_api_require_auth: bool,

    /// Database connection pool minimum size.
    #[serde(default = "default_pool_min_size")]
    pub pool_min_size: usize,

    /// Database connection pool maximum size.
    #[serde(default = "default_pool_max_size")]
    pub pool_max_size: usize,

    /// Database connection pool timeout in seconds.
    #[serde(default = "default_pool_timeout")]
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
    pub rate_limiting: Option<RateLimitingConfig>,

    /// Observer runtime configuration (optional, requires `observers` feature).
    #[cfg(feature = "observers")]
    #[serde(default)]
    pub observers: Option<ObserverConfig>,
}

#[cfg(feature = "observers")]
fn default_observers_enabled() -> bool {
    true
}

#[cfg(feature = "observers")]
fn default_poll_interval_ms() -> u64 {
    100
}

#[cfg(feature = "observers")]
fn default_batch_size() -> usize {
    100
}

#[cfg(feature = "observers")]
fn default_channel_capacity() -> usize {
    1000
}

#[cfg(feature = "observers")]
fn default_auto_reload() -> bool {
    true
}

#[cfg(feature = "observers")]
fn default_reload_interval_secs() -> u64 {
    60
}

/// Observer runtime configuration.
#[cfg(feature = "observers")]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ObserverConfig {
    /// Enable observer runtime (default: true).
    #[serde(default = "default_observers_enabled")]
    pub enabled: bool,

    /// Poll interval for change log in milliseconds (default: 100).
    #[serde(default = "default_poll_interval_ms")]
    pub poll_interval_ms: u64,

    /// Batch size for fetching change log entries (default: 100).
    #[serde(default = "default_batch_size")]
    pub batch_size: usize,

    /// Channel capacity for event buffering (default: 1000).
    #[serde(default = "default_channel_capacity")]
    pub channel_capacity: usize,

    /// Auto-reload observers on changes (default: true).
    #[serde(default = "default_auto_reload")]
    pub auto_reload: bool,

    /// Reload interval in seconds (default: 60).
    #[serde(default = "default_reload_interval_secs")]
    pub reload_interval_secs: u64,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            schema_path: default_schema_path(),
            database_url: default_database_url(),
            bind_addr: default_bind_addr(),
            cors_enabled: true,
            cors_origins: Vec::new(),
            compression_enabled: true,
            tracing_enabled: true,
            apq_enabled: true,
            cache_enabled: true,
            graphql_path: default_graphql_path(),
            health_path: default_health_path(),
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
            introspection_enabled: false, // Disabled by default for security
            introspection_require_auth: true, // Require auth when enabled
            design_api_require_auth: true, // Require auth for design endpoints
            pool_min_size: default_pool_min_size(),
            pool_max_size: default_pool_max_size(),
            pool_timeout_secs: default_pool_timeout(),
            auth: None,         // No auth by default
            tls: None,          // TLS disabled by default
            database_tls: None, // Database TLS disabled by default
            rate_limiting: None, // Rate limiting uses defaults
            #[cfg(feature = "observers")]
            observers: None, // Observers disabled by default
        }
    }
}

impl ServerConfig {
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
                         Set FRAISEQL_ADMIN_TOKEN or admin_token in config.".to_string());
                },
                Some(token) if token.len() < 32 => {
                    return Err("admin_token must be at least 32 characters for security.".to_string());
                },
                Some(_) => {},
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
                return Err(
                    "playground_enabled is true in production mode. \
                     Disable the playground or set FRAISEQL_ENV=development. \
                     The playground exposes sensitive schema information."
                        .to_string(),
                );
            }

            // CORS origins must be explicitly configured in production
            if self.cors_enabled && self.cors_origins.is_empty() {
                return Err(
                    "cors_enabled is true but cors_origins is empty in production mode. \
                     This allows requests from ANY origin, which is a security risk. \
                     Explicitly configure cors_origins with your allowed domains, \
                     or disable CORS and set FRAISEQL_ENV=development to bypass this check."
                        .to_string(),
                );
            }
        }

        Ok(())
    }

    /// Check if authentication is enabled.
    #[must_use]
    pub fn auth_enabled(&self) -> bool {
        self.auth.is_some()
    }
}

fn default_schema_path() -> PathBuf {
    PathBuf::from("schema.compiled.json")
}

fn default_database_url() -> String {
    "postgresql://localhost/fraiseql".to_string()
}

fn default_bind_addr() -> SocketAddr {
    "127.0.0.1:8000".parse().unwrap()
}

fn default_true() -> bool {
    true
}

fn default_graphql_path() -> String {
    "/graphql".to_string()
}

fn default_health_path() -> String {
    "/health".to_string()
}

fn default_introspection_path() -> String {
    "/introspection".to_string()
}

fn default_metrics_path() -> String {
    "/metrics".to_string()
}

fn default_metrics_json_path() -> String {
    "/metrics/json".to_string()
}

fn default_playground_path() -> String {
    "/playground".to_string()
}

fn default_subscription_path() -> String {
    "/ws".to_string()
}

fn default_pool_min_size() -> usize {
    5
}

fn default_pool_max_size() -> usize {
    20
}

fn default_pool_timeout() -> u64 {
    30
}

fn default_tls_min_version() -> String {
    "1.2".to_string()
}

fn default_postgres_ssl_mode() -> String {
    "prefer".to_string()
}

fn default_redis_ssl() -> bool {
    false
}

fn default_clickhouse_https() -> bool {
    false
}

fn default_elasticsearch_https() -> bool {
    false
}

fn default_verify_certs() -> bool {
    true
}

fn default_rate_limiting_enabled() -> bool {
    true
}

fn default_rate_limit_rps_per_ip() -> u32 {
    100
}

fn default_rate_limit_rps_per_user() -> u32 {
    1000
}

fn default_rate_limit_burst_size() -> u32 {
    500
}

fn default_rate_limit_cleanup_interval() -> u64 {
    300
}

#[cfg(test)]
mod tests {
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
        let json = serde_json::to_string(&config).expect("serialize should work");
        let restored: ServerConfig = serde_json::from_str(&json).expect("deserialize should work");

        assert_eq!(restored.subscription_path, config.subscription_path);
        assert_eq!(restored.subscriptions_enabled, config.subscriptions_enabled);
    }

    #[test]
    fn test_subscription_config_with_partial_toml() {
        let toml_str = r#"
            subscription_path = "/graphql-ws"
            subscriptions_enabled = false
        "#;

        let decoded: ServerConfig = toml::from_str(toml_str).expect("decode should work");
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

        let json = serde_json::to_string(&db_tls).expect("serialize should work");
        let restored: DatabaseTlsConfig =
            serde_json::from_str(&json).expect("deserialize should work");

        assert_eq!(restored.postgres_ssl_mode, db_tls.postgres_ssl_mode);
        assert_eq!(restored.redis_ssl, db_tls.redis_ssl);
        assert_eq!(restored.clickhouse_https, db_tls.clickhouse_https);
        assert_eq!(restored.elasticsearch_https, db_tls.elasticsearch_https);
        assert_eq!(restored.ca_bundle_path, db_tls.ca_bundle_path);
    }

    #[test]
    fn test_admin_api_disabled_by_default() {
        let config = ServerConfig::default();
        assert!(!config.admin_api_enabled, "Admin API should be disabled by default for security");
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
}
