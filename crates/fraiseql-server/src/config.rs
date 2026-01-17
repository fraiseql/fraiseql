//! Server configuration.

use fraiseql_core::security::OidcConfig;
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use std::path::PathBuf;

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

    /// Enable GraphQL playground/IDE.
    ///
    /// When enabled, serves a GraphQL IDE (GraphiQL or Apollo Sandbox)
    /// at the configured `playground_path`.
    #[serde(default = "default_true")]
    pub playground_enabled: bool,

    /// Which GraphQL IDE to use.
    ///
    /// - `graphiql`: The classic GraphQL IDE (default)
    /// - `apollo-sandbox`: Apollo's embeddable sandbox
    #[serde(default)]
    pub playground_tool: PlaygroundTool,

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
            playground_enabled: true,
            playground_tool: PlaygroundTool::default(),
            metrics_enabled: false, // Disabled by default for security
            metrics_token: None,
            pool_min_size: default_pool_min_size(),
            pool_max_size: default_pool_max_size(),
            pool_timeout_secs: default_pool_timeout(),
            auth: None, // No auth by default
        }
    }
}

impl ServerConfig {
    /// Validate configuration.
    ///
    /// # Errors
    ///
    /// Returns error if:
    /// - `metrics_enabled` is true but `metrics_token` is not set
    /// - `metrics_token` is set but too short (< 16 characters)
    /// - `auth` config is set but invalid (e.g., empty issuer)
    pub fn validate(&self) -> Result<(), String> {
        if self.metrics_enabled {
            match &self.metrics_token {
                None => {
                    return Err(
                        "metrics_enabled is true but metrics_token is not set. \
                         Set FRAISEQL_METRICS_TOKEN or metrics_token in config."
                            .to_string(),
                    );
                }
                Some(token) if token.len() < 16 => {
                    return Err(
                        "metrics_token must be at least 16 characters for security."
                            .to_string(),
                    );
                }
                Some(_) => {}
            }
        }

        // Validate OIDC config if present
        if let Some(ref auth) = self.auth {
            auth.validate().map_err(|e| e.to_string())?;
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

fn default_pool_min_size() -> usize {
    5
}

fn default_pool_max_size() -> usize {
    20
}

fn default_pool_timeout() -> u64 {
    30
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
        let config = ServerConfig::default();
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
            ..ServerConfig::default()
        };
        assert!(config.validate().is_ok());
    }
}
