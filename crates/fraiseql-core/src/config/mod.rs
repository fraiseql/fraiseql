//! Configuration management.
//!
//! This module provides comprehensive configuration for FraiseQL servers:
//!
//! - **Server**: Host, port, worker threads
//! - **Database**: Connection URL, pool settings, timeouts
//! - **CORS**: Allowed origins, methods, headers
//! - **Auth**: JWT/Auth0/Clerk configuration
//! - **Rate Limiting**: Request limits per window
//! - **Caching**: APQ and response caching settings
//!
//! # Configuration File Format
//!
//! FraiseQL supports TOML configuration files:
//!
//! ```toml
//! [server]
//! host = "0.0.0.0"
//! port = 8000
//!
//! [database]
//! url = "postgresql://localhost/mydb"
//! max_connections = 10
//! timeout_secs = 30
//!
//! [cors]
//! allowed_origins = ["http://localhost:3000"]
//! allow_credentials = true
//!
//! [auth]
//! provider = "jwt"
//! secret = "${JWT_SECRET}"
//!
//! [rate_limit]
//! requests_per_minute = 100
//! ```
//!
//! # Environment Variable Expansion
//!
//! Config values can reference environment variables using `${VAR}` syntax.
//! This is especially useful for secrets that shouldn't be in config files.

use crate::error::{FraiseQLError, Result};
use serde::{Deserialize, Serialize};
use std::path::Path;

// =============================================================================
// Server Configuration
// =============================================================================

/// Server-specific configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ServerConfig {
    /// Host to bind to.
    pub host: String,

    /// Port to bind to.
    pub port: u16,

    /// Number of worker threads (0 = auto).
    pub workers: usize,

    /// Request body size limit in bytes.
    pub max_body_size: usize,

    /// Enable request logging.
    pub request_logging: bool,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            host: "0.0.0.0".to_string(),
            port: 8000,
            workers: 0,                 // Auto-detect
            max_body_size: 1024 * 1024, // 1MB
            request_logging: true,
        }
    }
}

// =============================================================================
// Database Configuration
// =============================================================================

/// Database connection configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct DatabaseConfig {
    /// `PostgreSQL` connection URL.
    pub url: String,

    /// Maximum connections in pool.
    pub max_connections: u32,

    /// Minimum connections to maintain.
    pub min_connections: u32,

    /// Connection timeout in seconds.
    pub connect_timeout_secs: u64,

    /// Query timeout in seconds.
    pub query_timeout_secs: u64,

    /// Idle timeout in seconds (0 = no timeout).
    pub idle_timeout_secs: u64,

    /// Enable SSL for database connections.
    pub ssl_mode: SslMode,
}

impl Default for DatabaseConfig {
    fn default() -> Self {
        Self {
            url: String::new(),
            max_connections: 10,
            min_connections: 1,
            connect_timeout_secs: 10,
            query_timeout_secs: 30,
            idle_timeout_secs: 600,
            ssl_mode: SslMode::Prefer,
        }
    }
}

/// SSL mode for database connections.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum SslMode {
    /// Disable SSL.
    Disable,
    /// Prefer SSL but allow non-SSL.
    #[default]
    Prefer,
    /// Require SSL.
    Require,
    /// Require SSL and verify CA.
    VerifyCa,
    /// Require SSL and verify full certificate.
    VerifyFull,
}

// =============================================================================
// CORS Configuration
// =============================================================================

/// Cross-Origin Resource Sharing (CORS) configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct CorsConfig {
    /// Enabled CORS.
    pub enabled: bool,

    /// Allowed origins. Empty = allow all, "*" = allow any.
    pub allowed_origins: Vec<String>,

    /// Allowed HTTP methods.
    pub allowed_methods: Vec<String>,

    /// Allowed headers.
    pub allowed_headers: Vec<String>,

    /// Headers to expose to the client.
    pub expose_headers: Vec<String>,

    /// Allow credentials (cookies, authorization headers).
    pub allow_credentials: bool,

    /// Preflight cache duration in seconds.
    pub max_age_secs: u64,
}

impl Default for CorsConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            allowed_origins: vec![], // Empty = allow all
            allowed_methods: vec!["GET".to_string(), "POST".to_string(), "OPTIONS".to_string()],
            allowed_headers: vec![
                "Content-Type".to_string(),
                "Authorization".to_string(),
                "X-Request-ID".to_string(),
            ],
            expose_headers: vec![],
            allow_credentials: false,
            max_age_secs: 86400, // 24 hours
        }
    }
}

// =============================================================================
// Authentication Configuration
// =============================================================================

/// Authentication configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AuthConfig {
    /// Enable authentication.
    pub enabled: bool,

    /// Authentication provider.
    pub provider: AuthProvider,

    /// JWT secret (for jwt provider).
    pub jwt_secret: Option<String>,

    /// JWT algorithm (default: HS256).
    pub jwt_algorithm: String,

    /// Auth0/Clerk domain.
    pub domain: Option<String>,

    /// Auth0/Clerk audience.
    pub audience: Option<String>,

    /// Auth0/Clerk client ID.
    pub client_id: Option<String>,

    /// Header name for auth token.
    pub header_name: String,

    /// Token prefix (e.g., "Bearer ").
    pub token_prefix: String,

    /// Paths to exclude from authentication.
    pub exclude_paths: Vec<String>,
}

impl Default for AuthConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            provider: AuthProvider::None,
            jwt_secret: None,
            jwt_algorithm: "HS256".to_string(),
            domain: None,
            audience: None,
            client_id: None,
            header_name: "Authorization".to_string(),
            token_prefix: "Bearer ".to_string(),
            exclude_paths: vec!["/health".to_string()],
        }
    }
}

/// Authentication provider.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum AuthProvider {
    /// No authentication.
    #[default]
    None,
    /// Simple JWT authentication.
    Jwt,
    /// Auth0 authentication.
    Auth0,
    /// Clerk authentication.
    Clerk,
    /// Custom webhook-based authentication.
    Webhook,
}

// =============================================================================
// Rate Limiting Configuration
// =============================================================================

/// Rate limiting configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct RateLimitConfig {
    /// Enable rate limiting.
    pub enabled: bool,

    /// Maximum requests per window.
    pub requests_per_window: u32,

    /// Window duration in seconds.
    pub window_secs: u64,

    /// Key extractor (ip, user, `api_key`).
    pub key_by: RateLimitKey,

    /// Paths to exclude from rate limiting.
    pub exclude_paths: Vec<String>,

    /// Custom limits per path pattern.
    pub path_limits: Vec<PathRateLimit>,
}

impl Default for RateLimitConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            requests_per_window: 100,
            window_secs: 60,
            key_by: RateLimitKey::Ip,
            exclude_paths: vec!["/health".to_string()],
            path_limits: vec![],
        }
    }
}

/// Rate limit key extractor.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RateLimitKey {
    /// Rate limit by IP address.
    #[default]
    Ip,
    /// Rate limit by authenticated user.
    User,
    /// Rate limit by API key.
    ApiKey,
}

/// Per-path rate limit override.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PathRateLimit {
    /// Path pattern (glob).
    pub path: String,
    /// Maximum requests per window for this path.
    pub requests_per_window: u32,
}

// =============================================================================
// Caching Configuration
// =============================================================================

/// Caching configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct CacheConfig {
    /// Enable Automatic Persisted Queries (APQ).
    pub apq_enabled: bool,

    /// APQ cache TTL in seconds.
    pub apq_ttl_secs: u64,

    /// Maximum APQ cache entries.
    pub apq_max_entries: usize,

    /// Enable response caching.
    pub response_cache_enabled: bool,

    /// Response cache TTL in seconds.
    pub response_cache_ttl_secs: u64,

    /// Maximum response cache entries.
    pub response_cache_max_entries: usize,
}

impl Default for CacheConfig {
    fn default() -> Self {
        Self {
            apq_enabled: true,
            apq_ttl_secs: 86400, // 24 hours
            apq_max_entries: 10_000,
            response_cache_enabled: false,
            response_cache_ttl_secs: 60,
            response_cache_max_entries: 1_000,
        }
    }
}

// =============================================================================
// Collation Configuration
// =============================================================================

/// Collation configuration for user-aware sorting.
///
/// This configuration enables automatic collation support based on user locale,
/// adapting to database capabilities.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct CollationConfig {
    /// Enable automatic user-aware collation.
    pub enabled: bool,

    /// Fallback locale for unauthenticated users.
    pub fallback_locale: String,

    /// Allowed locales (whitelist for security).
    pub allowed_locales: Vec<String>,

    /// Strategy when user locale is not in allowed list.
    pub on_invalid_locale: InvalidLocaleStrategy,

    /// Database-specific overrides (optional).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub database_overrides: Option<DatabaseCollationOverrides>,
}

impl Default for CollationConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            fallback_locale: "en-US".to_string(),
            allowed_locales: vec![
                "en-US".into(),
                "en-GB".into(),
                "fr-FR".into(),
                "de-DE".into(),
                "es-ES".into(),
                "ja-JP".into(),
                "zh-CN".into(),
                "pt-BR".into(),
                "it-IT".into(),
            ],
            on_invalid_locale: InvalidLocaleStrategy::Fallback,
            database_overrides: None,
        }
    }
}

/// Strategy when user locale is not in allowed list.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum InvalidLocaleStrategy {
    /// Use fallback locale.
    #[default]
    Fallback,
    /// Use database default (no COLLATE clause).
    DatabaseDefault,
    /// Return error.
    Error,
}

/// Database-specific collation overrides.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatabaseCollationOverrides {
    /// PostgreSQL-specific settings.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub postgres: Option<PostgresCollationConfig>,

    /// MySQL-specific settings.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mysql: Option<MySqlCollationConfig>,

    /// SQLite-specific settings.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sqlite: Option<SqliteCollationConfig>,

    /// SQL Server-specific settings.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sqlserver: Option<SqlServerCollationConfig>,
}

/// PostgreSQL-specific collation configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PostgresCollationConfig {
    /// Use ICU collations (recommended).
    pub use_icu: bool,

    /// Provider: "icu" or "libc".
    pub provider: String,
}

impl Default for PostgresCollationConfig {
    fn default() -> Self {
        Self {
            use_icu: true,
            provider: "icu".to_string(),
        }
    }
}

/// MySQL-specific collation configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MySqlCollationConfig {
    /// Charset (e.g., "utf8mb4").
    pub charset: String,

    /// Collation suffix (e.g., "_unicode_ci" or "_0900_ai_ci").
    pub suffix: String,
}

impl Default for MySqlCollationConfig {
    fn default() -> Self {
        Self {
            charset: "utf8mb4".to_string(),
            suffix: "_unicode_ci".to_string(),
        }
    }
}

/// SQLite-specific collation configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SqliteCollationConfig {
    /// Use COLLATE NOCASE for case-insensitive sorting.
    pub use_nocase: bool,
}

impl Default for SqliteCollationConfig {
    fn default() -> Self {
        Self { use_nocase: true }
    }
}

/// SQL Server-specific collation configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SqlServerCollationConfig {
    /// Case-insensitive (CI) collations.
    pub case_insensitive: bool,

    /// Accent-insensitive (AI) collations.
    pub accent_insensitive: bool,
}

impl Default for SqlServerCollationConfig {
    fn default() -> Self {
        Self {
            case_insensitive: true,
            accent_insensitive: true,
        }
    }
}

// =============================================================================
// Main Configuration
// =============================================================================

/// Main configuration structure.
///
/// This is the complete configuration for a `FraiseQL` server instance.
/// It can be loaded from a TOML file, environment variables, or built programmatically.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct FraiseQLConfig {
    /// Server configuration.
    pub server: ServerConfig,

    /// Database configuration.
    pub database: DatabaseConfig,

    /// CORS configuration.
    pub cors: CorsConfig,

    /// Authentication configuration.
    pub auth: AuthConfig,

    /// Rate limiting configuration.
    pub rate_limit: RateLimitConfig,

    /// Caching configuration.
    pub cache: CacheConfig,

    /// Collation configuration.
    pub collation: CollationConfig,

    // Legacy fields for backward compatibility
    #[serde(skip)]
    database_url_compat: Option<String>,

    /// Database connection URL (legacy, prefer database.url).
    #[serde(skip_serializing, default)]
    pub database_url: String,

    /// Server host (legacy, prefer server.host).
    #[serde(skip_serializing, default)]
    pub host: String,

    /// Server port (legacy, prefer server.port).
    #[serde(skip_serializing, default)]
    pub port: u16,

    /// Maximum connections (legacy, prefer `database.max_connections`).
    #[serde(skip_serializing, default)]
    pub max_connections: u32,

    /// Query timeout (legacy, prefer `database.query_timeout_secs`).
    #[serde(skip_serializing, default)]
    pub query_timeout_secs: u64,
}

impl Default for FraiseQLConfig {
    fn default() -> Self {
        let server = ServerConfig::default();
        let database = DatabaseConfig::default();

        Self {
            // Legacy compat fields
            database_url: String::new(),
            host: server.host.clone(),
            port: server.port,
            max_connections: database.max_connections,
            query_timeout_secs: database.query_timeout_secs,
            database_url_compat: None,

            // New structured config
            server,
            database,
            cors: CorsConfig::default(),
            auth: AuthConfig::default(),
            rate_limit: RateLimitConfig::default(),
            cache: CacheConfig::default(),
            collation: CollationConfig::default(),
        }
    }
}

impl FraiseQLConfig {
    /// Create a new configuration builder.
    #[must_use]
    pub fn builder() -> ConfigBuilder {
        ConfigBuilder::default()
    }

    /// Load configuration from a TOML file.
    ///
    /// # Errors
    ///
    /// Returns error if the file cannot be read or parsed.
    pub fn from_file<P: AsRef<Path>>(path: P) -> Result<Self> {
        let path = path.as_ref();
        let content = std::fs::read_to_string(path).map_err(|e| FraiseQLError::Configuration {
            message: format!("Failed to read config file '{}': {}", path.display(), e),
        })?;

        Self::from_toml(&content)
    }

    /// Load configuration from a TOML string.
    ///
    /// # Errors
    ///
    /// Returns error if the TOML is invalid.
    pub fn from_toml(content: &str) -> Result<Self> {
        // Expand environment variables in the content
        let expanded = expand_env_vars(content);

        let mut config: Self =
            toml::from_str(&expanded).map_err(|e| FraiseQLError::Configuration {
                message: format!("Invalid TOML configuration: {e}"),
            })?;

        // Sync legacy fields
        config.sync_legacy_fields();

        Ok(config)
    }

    /// Load configuration from environment variables.
    ///
    /// # Errors
    ///
    /// Returns error if required environment variables are missing.
    pub fn from_env() -> Result<Self> {
        let database_url =
            std::env::var("DATABASE_URL").map_err(|_| FraiseQLError::Configuration {
                message: "DATABASE_URL not set".to_string(),
            })?;

        let host = std::env::var("FRAISEQL_HOST").unwrap_or_else(|_| "0.0.0.0".to_string());

        let port = std::env::var("FRAISEQL_PORT")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(8000);

        let max_connections = std::env::var("FRAISEQL_MAX_CONNECTIONS")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(10);

        let query_timeout = std::env::var("FRAISEQL_QUERY_TIMEOUT")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(30);

        let mut config = Self {
            server: ServerConfig {
                host: host.clone(),
                port,
                ..Default::default()
            },
            database: DatabaseConfig {
                url: database_url.clone(),
                max_connections,
                query_timeout_secs: query_timeout,
                ..Default::default()
            },
            // Legacy compat
            database_url,
            host,
            port,
            max_connections,
            query_timeout_secs: query_timeout,
            ..Default::default()
        };

        // Load optional auth settings from env
        if let Ok(provider) = std::env::var("FRAISEQL_AUTH_PROVIDER") {
            config.auth.enabled = true;
            config.auth.provider = match provider.to_lowercase().as_str() {
                "jwt" => AuthProvider::Jwt,
                "auth0" => AuthProvider::Auth0,
                "clerk" => AuthProvider::Clerk,
                "webhook" => AuthProvider::Webhook,
                _ => AuthProvider::None,
            };
        }

        if let Ok(secret) = std::env::var("JWT_SECRET") {
            config.auth.jwt_secret = Some(secret);
        }

        if let Ok(domain) = std::env::var("AUTH0_DOMAIN") {
            config.auth.domain = Some(domain);
        }

        if let Ok(audience) = std::env::var("AUTH0_AUDIENCE") {
            config.auth.audience = Some(audience);
        }

        Ok(config)
    }

    /// Sync legacy flat fields with new structured fields.
    fn sync_legacy_fields(&mut self) {
        // If structured database.url is set, use it for legacy field
        if !self.database.url.is_empty() {
            self.database_url = self.database.url.clone();
        } else if !self.database_url.is_empty() {
            // If legacy field is set, copy to structured
            self.database.url = self.database_url.clone();
        }

        // Sync server fields
        self.host = self.server.host.clone();
        self.port = self.server.port;
        self.max_connections = self.database.max_connections;
        self.query_timeout_secs = self.database.query_timeout_secs;
    }

    /// Create a test configuration.
    #[must_use]
    pub fn test() -> Self {
        Self {
            server: ServerConfig {
                host: "127.0.0.1".to_string(),
                port: 0, // Random port
                ..Default::default()
            },
            database: DatabaseConfig {
                url: "postgresql://postgres:postgres@localhost:5432/fraiseql_test".to_string(),
                max_connections: 2,
                query_timeout_secs: 5,
                ..Default::default()
            },
            // Legacy compat
            database_url: "postgresql://postgres:postgres@localhost:5432/fraiseql_test".to_string(),
            host: "127.0.0.1".to_string(),
            port: 0,
            max_connections: 2,
            query_timeout_secs: 5,
            ..Default::default()
        }
    }

    /// Validate the configuration.
    ///
    /// # Errors
    ///
    /// Returns error if configuration is invalid.
    pub fn validate(&self) -> Result<()> {
        // Database URL required
        if self.database.url.is_empty() && self.database_url.is_empty() {
            return Err(FraiseQLError::Configuration {
                message: "database.url is required".to_string(),
            });
        }

        // Validate auth config
        if self.auth.enabled {
            match self.auth.provider {
                AuthProvider::Jwt => {
                    if self.auth.jwt_secret.is_none() {
                        return Err(FraiseQLError::Configuration {
                            message: "auth.jwt_secret is required when using JWT provider"
                                .to_string(),
                        });
                    }
                }
                AuthProvider::Auth0 | AuthProvider::Clerk => {
                    if self.auth.domain.is_none() {
                        return Err(FraiseQLError::Configuration {
                            message: format!(
                                "auth.domain is required when using {:?} provider",
                                self.auth.provider
                            ),
                        });
                    }
                }
                AuthProvider::Webhook | AuthProvider::None => {}
            }
        }

        Ok(())
    }

    /// Export configuration to TOML string.
    #[must_use]
    pub fn to_toml(&self) -> String {
        toml::to_string_pretty(self).unwrap_or_default()
    }
}

/// Expand environment variables in a string.
///
/// Supports `${VAR}` and `$VAR` syntax.
#[allow(clippy::expect_used)]
fn expand_env_vars(content: &str) -> String {
    use once_cell::sync::Lazy;

    // The regex pattern is a compile-time constant and is guaranteed to be valid
    static ENV_VAR_REGEX: Lazy<regex::Regex> =
        Lazy::new(|| regex::Regex::new(r"\$\{([A-Za-z_][A-Za-z0-9_]*)\}").expect("Invalid regex"));

    let mut result = content.to_string();

    for cap in ENV_VAR_REGEX.captures_iter(content) {
        if let Some(full_match) = cap.get(0) {
            if let Some(var_name_match) = cap.get(1) {
                let full_match_str = full_match.as_str();
                let var_name = var_name_match.as_str();

                if let Ok(value) = std::env::var(var_name) {
                    result = result.replace(full_match_str, &value);
                }
            }
        }
    }

    result
}

/// Configuration builder.
#[derive(Debug, Default)]
pub struct ConfigBuilder {
    config: FraiseQLConfig,
}

impl ConfigBuilder {
    /// Set the database URL.
    #[must_use]
    pub fn database_url(mut self, url: &str) -> Self {
        self.config.database.url = url.to_string();
        self.config.database_url = url.to_string();
        self
    }

    /// Set the server host.
    #[must_use]
    pub fn host(mut self, host: &str) -> Self {
        self.config.server.host = host.to_string();
        self.config.host = host.to_string();
        self
    }

    /// Set the server port.
    #[must_use]
    pub fn port(mut self, port: u16) -> Self {
        self.config.server.port = port;
        self.config.port = port;
        self
    }

    /// Set maximum database connections.
    #[must_use]
    pub fn max_connections(mut self, n: u32) -> Self {
        self.config.database.max_connections = n;
        self.config.max_connections = n;
        self
    }

    /// Set query timeout.
    #[must_use]
    pub fn query_timeout(mut self, secs: u64) -> Self {
        self.config.database.query_timeout_secs = secs;
        self.config.query_timeout_secs = secs;
        self
    }

    /// Set CORS configuration.
    #[must_use]
    pub fn cors(mut self, cors: CorsConfig) -> Self {
        self.config.cors = cors;
        self
    }

    /// Set auth configuration.
    #[must_use]
    pub fn auth(mut self, auth: AuthConfig) -> Self {
        self.config.auth = auth;
        self
    }

    /// Set rate limit configuration.
    #[must_use]
    pub fn rate_limit(mut self, rate_limit: RateLimitConfig) -> Self {
        self.config.rate_limit = rate_limit;
        self
    }

    /// Set cache configuration.
    #[must_use]
    pub fn cache(mut self, cache: CacheConfig) -> Self {
        self.config.cache = cache;
        self
    }

    /// Set collation configuration.
    #[must_use]
    pub fn collation(mut self, collation: CollationConfig) -> Self {
        self.config.collation = collation;
        self
    }

    /// Build the configuration.
    ///
    /// # Errors
    ///
    /// Returns error if configuration is invalid.
    pub fn build(mut self) -> Result<FraiseQLConfig> {
        self.config.sync_legacy_fields();
        self.config.validate()?;
        Ok(self.config)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = FraiseQLConfig::default();
        assert_eq!(config.port, 8000);
        assert_eq!(config.host, "0.0.0.0");
        assert_eq!(config.server.port, 8000);
        assert_eq!(config.server.host, "0.0.0.0");
    }

    #[test]
    fn test_builder() {
        let config = FraiseQLConfig::builder()
            .database_url("postgresql://localhost/test")
            .port(9000)
            .build()
            .unwrap();

        assert_eq!(config.port, 9000);
        assert_eq!(config.server.port, 9000);
        assert!(!config.database_url.is_empty());
        assert!(!config.database.url.is_empty());
    }

    #[test]
    fn test_builder_requires_database_url() {
        let result = FraiseQLConfig::builder().build();
        assert!(result.is_err());
    }

    #[test]
    fn test_from_toml_minimal() {
        let toml = r#"
[database]
url = "postgresql://localhost/test"
"#;
        let config = FraiseQLConfig::from_toml(toml).unwrap();
        assert_eq!(config.database.url, "postgresql://localhost/test");
        assert_eq!(config.database_url, "postgresql://localhost/test");
    }

    #[test]
    fn test_from_toml_full() {
        let toml = r#"
[server]
host = "127.0.0.1"
port = 9000
workers = 4
max_body_size = 2097152
request_logging = true

[database]
url = "postgresql://localhost/mydb"
max_connections = 20
min_connections = 2
connect_timeout_secs = 15
query_timeout_secs = 60
idle_timeout_secs = 300
ssl_mode = "require"

[cors]
enabled = true
allowed_origins = ["http://localhost:3000", "https://app.example.com"]
allow_credentials = true

[auth]
enabled = true
provider = "jwt"
jwt_secret = "my-secret-key"
jwt_algorithm = "HS256"
exclude_paths = ["/health", "/metrics"]

[rate_limit]
enabled = true
requests_per_window = 200
window_secs = 120
key_by = "user"

[cache]
apq_enabled = true
apq_ttl_secs = 3600
response_cache_enabled = true
"#;
        let config = FraiseQLConfig::from_toml(toml).unwrap();

        // Server
        assert_eq!(config.server.host, "127.0.0.1");
        assert_eq!(config.server.port, 9000);
        assert_eq!(config.server.workers, 4);

        // Database
        assert_eq!(config.database.url, "postgresql://localhost/mydb");
        assert_eq!(config.database.max_connections, 20);
        assert_eq!(config.database.ssl_mode, SslMode::Require);

        // CORS
        assert!(config.cors.enabled);
        assert_eq!(config.cors.allowed_origins.len(), 2);
        assert!(config.cors.allow_credentials);

        // Auth
        assert!(config.auth.enabled);
        assert_eq!(config.auth.provider, AuthProvider::Jwt);
        assert_eq!(config.auth.jwt_secret, Some("my-secret-key".to_string()));

        // Rate Limit
        assert!(config.rate_limit.enabled);
        assert_eq!(config.rate_limit.requests_per_window, 200);
        assert_eq!(config.rate_limit.key_by, RateLimitKey::User);

        // Cache
        assert!(config.cache.apq_enabled);
        assert!(config.cache.response_cache_enabled);
    }

    #[test]
    fn test_env_var_expansion() {
        std::env::set_var("TEST_DB_URL", "postgresql://user:pass@host/db");
        std::env::set_var("TEST_JWT_SECRET", "super-secret");

        let toml = r#"
[database]
url = "${TEST_DB_URL}"

[auth]
enabled = true
provider = "jwt"
jwt_secret = "${TEST_JWT_SECRET}"
"#;
        let config = FraiseQLConfig::from_toml(toml).unwrap();

        assert_eq!(config.database.url, "postgresql://user:pass@host/db");
        assert_eq!(config.auth.jwt_secret, Some("super-secret".to_string()));

        std::env::remove_var("TEST_DB_URL");
        std::env::remove_var("TEST_JWT_SECRET");
    }

    #[test]
    fn test_auth_validation_jwt_requires_secret() {
        let toml = r#"
[database]
url = "postgresql://localhost/test"

[auth]
enabled = true
provider = "jwt"
"#;
        let result = FraiseQLConfig::from_toml(toml);
        // from_toml succeeds but validate would fail
        let config = result.unwrap();
        let validation = config.validate();
        assert!(validation.is_err());
        assert!(validation
            .unwrap_err()
            .to_string()
            .contains("jwt_secret is required"));
    }

    #[test]
    fn test_auth_validation_auth0_requires_domain() {
        let toml = r#"
[database]
url = "postgresql://localhost/test"

[auth]
enabled = true
provider = "auth0"
"#;
        let config = FraiseQLConfig::from_toml(toml).unwrap();
        let validation = config.validate();
        assert!(validation.is_err());
        assert!(validation
            .unwrap_err()
            .to_string()
            .contains("domain is required"));
    }

    #[test]
    fn test_to_toml() {
        let config = FraiseQLConfig::builder()
            .database_url("postgresql://localhost/test")
            .port(9000)
            .build()
            .unwrap();

        let toml_str = config.to_toml();
        assert!(toml_str.contains("[server]"));
        assert!(toml_str.contains("[database]"));
        assert!(toml_str.contains("port = 9000"));
    }

    #[test]
    fn test_cors_config_defaults() {
        let cors = CorsConfig::default();
        assert!(cors.enabled);
        assert!(cors.allowed_origins.is_empty()); // Empty = allow all
        assert!(cors.allowed_methods.contains(&"POST".to_string()));
        assert!(cors.allowed_headers.contains(&"Authorization".to_string()));
    }

    #[test]
    fn test_rate_limit_key_variants() {
        let toml = r#"
[database]
url = "postgresql://localhost/test"

[rate_limit]
key_by = "api_key"
"#;
        let config = FraiseQLConfig::from_toml(toml).unwrap();
        assert_eq!(config.rate_limit.key_by, RateLimitKey::ApiKey);
    }

    #[test]
    fn test_ssl_mode_variants() {
        for (ssl_str, expected) in [
            ("disable", SslMode::Disable),
            ("prefer", SslMode::Prefer),
            ("require", SslMode::Require),
            ("verify-ca", SslMode::VerifyCa),
            ("verify-full", SslMode::VerifyFull),
        ] {
            let toml = format!(
                r#"
[database]
url = "postgresql://localhost/test"
ssl_mode = "{}"
"#,
                ssl_str
            );
            let config = FraiseQLConfig::from_toml(&toml).unwrap();
            assert_eq!(config.database.ssl_mode, expected);
        }
    }

    #[test]
    fn test_legacy_field_sync() {
        let config = FraiseQLConfig::builder()
            .database_url("postgresql://localhost/test")
            .host("192.168.1.1")
            .port(4000)
            .max_connections(50)
            .query_timeout(120)
            .build()
            .unwrap();

        // Both legacy and new fields should match
        assert_eq!(config.host, "192.168.1.1");
        assert_eq!(config.server.host, "192.168.1.1");
        assert_eq!(config.port, 4000);
        assert_eq!(config.server.port, 4000);
        assert_eq!(config.max_connections, 50);
        assert_eq!(config.database.max_connections, 50);
        assert_eq!(config.query_timeout_secs, 120);
        assert_eq!(config.database.query_timeout_secs, 120);
    }

    #[test]
    fn test_auth_providers() {
        for (provider_str, expected) in [
            ("none", AuthProvider::None),
            ("jwt", AuthProvider::Jwt),
            ("auth0", AuthProvider::Auth0),
            ("clerk", AuthProvider::Clerk),
            ("webhook", AuthProvider::Webhook),
        ] {
            let toml = format!(
                r#"
[database]
url = "postgresql://localhost/test"

[auth]
provider = "{}"
"#,
                provider_str
            );
            let config = FraiseQLConfig::from_toml(&toml).unwrap();
            assert_eq!(config.auth.provider, expected);
        }
    }

    #[test]
    fn test_collation_config_default() {
        let config = CollationConfig::default();
        assert!(config.enabled);
        assert_eq!(config.fallback_locale, "en-US");
        assert!(config.allowed_locales.contains(&"en-US".to_string()));
        assert!(config.allowed_locales.contains(&"fr-FR".to_string()));
        assert_eq!(config.on_invalid_locale, InvalidLocaleStrategy::Fallback);
        assert!(config.database_overrides.is_none());
    }

    #[test]
    fn test_collation_config_from_toml() {
        let toml = r#"
[database]
url = "postgresql://localhost/test"

[collation]
enabled = true
fallback_locale = "en-GB"
on_invalid_locale = "error"
allowed_locales = ["en-GB", "fr-FR", "de-DE"]
"#;
        let config = FraiseQLConfig::from_toml(toml).unwrap();

        assert!(config.collation.enabled);
        assert_eq!(config.collation.fallback_locale, "en-GB");
        assert_eq!(config.collation.on_invalid_locale, InvalidLocaleStrategy::Error);
        assert_eq!(config.collation.allowed_locales.len(), 3);
        assert!(config.collation.allowed_locales.contains(&"de-DE".to_string()));
    }

    #[test]
    fn test_collation_with_postgres_overrides() {
        let toml = r#"
[database]
url = "postgresql://localhost/test"

[collation]
enabled = true
fallback_locale = "en-US"

[collation.database_overrides.postgres]
use_icu = false
provider = "libc"
"#;
        let config = FraiseQLConfig::from_toml(toml).unwrap();

        let overrides = config.collation.database_overrides.as_ref().unwrap();
        let pg_config = overrides.postgres.as_ref().unwrap();
        assert!(!pg_config.use_icu);
        assert_eq!(pg_config.provider, "libc");
    }

    #[test]
    fn test_collation_with_mysql_overrides() {
        let toml = r#"
[database]
url = "postgresql://localhost/test"

[collation]
enabled = true

[collation.database_overrides.mysql]
charset = "utf8mb4"
suffix = "_0900_ai_ci"
"#;
        let config = FraiseQLConfig::from_toml(toml).unwrap();

        let overrides = config.collation.database_overrides.as_ref().unwrap();
        let mysql_config = overrides.mysql.as_ref().unwrap();
        assert_eq!(mysql_config.charset, "utf8mb4");
        assert_eq!(mysql_config.suffix, "_0900_ai_ci");
    }

    #[test]
    fn test_collation_with_sqlite_overrides() {
        let toml = r#"
[database]
url = "postgresql://localhost/test"

[collation]
enabled = true

[collation.database_overrides.sqlite]
use_nocase = false
"#;
        let config = FraiseQLConfig::from_toml(toml).unwrap();

        let overrides = config.collation.database_overrides.as_ref().unwrap();
        let sqlite_config = overrides.sqlite.as_ref().unwrap();
        assert!(!sqlite_config.use_nocase);
    }

    #[test]
    fn test_invalid_locale_strategy_variants() {
        for (strategy_str, expected) in [
            ("fallback", InvalidLocaleStrategy::Fallback),
            ("database_default", InvalidLocaleStrategy::DatabaseDefault),
            ("error", InvalidLocaleStrategy::Error),
        ] {
            let toml = format!(
                r#"
[database]
url = "postgresql://localhost/test"

[collation]
on_invalid_locale = "{}"
"#,
                strategy_str
            );
            let config = FraiseQLConfig::from_toml(&toml).unwrap();
            assert_eq!(config.collation.on_invalid_locale, expected);
        }
    }

    #[test]
    fn test_collation_disabled() {
        let toml = r#"
[database]
url = "postgresql://localhost/test"

[collation]
enabled = false
"#;
        let config = FraiseQLConfig::from_toml(toml).unwrap();
        assert!(!config.collation.enabled);
    }

    #[test]
    fn test_collation_config_builder() {
        let mut collation = CollationConfig::default();
        collation.enabled = false;
        collation.fallback_locale = "de-DE".to_string();

        let config = FraiseQLConfig::builder()
            .database_url("postgresql://localhost/test")
            .collation(collation)
            .build()
            .unwrap();

        assert!(!config.collation.enabled);
        assert_eq!(config.collation.fallback_locale, "de-DE");
    }
}
