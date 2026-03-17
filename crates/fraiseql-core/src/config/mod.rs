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

use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::error::{FraiseQLError, Result};

mod auth;
mod cache;
mod cors;
mod database;
mod rate_limit;
mod server;

pub use auth::{AuthConfig, AuthProvider};
pub use cache::CacheConfig;
pub use cors::CorsConfig;
pub use database::{DatabaseConfig, MutationTimingConfig, SslMode};
// =============================================================================
// Collation Configuration — re-exported from fraiseql-db
// =============================================================================
pub use fraiseql_db::{
    CollationConfig, DatabaseCollationOverrides, InvalidLocaleStrategy, MySqlCollationConfig,
    PostgresCollationConfig, SqlServerCollationConfig, SqliteCollationConfig,
};
pub use rate_limit::{PathRateLimit, RateLimitConfig, RateLimitKey};
pub use server::CoreServerConfig;

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
    pub server: CoreServerConfig,

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
        let server = CoreServerConfig::default();
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

        let port = std::env::var("FRAISEQL_PORT").ok().and_then(|s| s.parse().ok()).unwrap_or(8000);

        let max_connections = std::env::var("FRAISEQL_MAX_CONNECTIONS")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(10);

        let query_timeout = std::env::var("FRAISEQL_QUERY_TIMEOUT")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(30);

        let mut config = Self {
            server: CoreServerConfig {
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
    pub fn test() -> Self {
        Self {
            server: CoreServerConfig {
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

        // Pool invariants
        if self.database.max_connections == 0 {
            return Err(FraiseQLError::Configuration {
                message: "database.max_connections must be at least 1".to_string(),
            });
        }
        if self.database.min_connections > self.database.max_connections {
            return Err(FraiseQLError::Configuration {
                message: format!(
                    "database.min_connections ({}) must not exceed max_connections ({})",
                    self.database.min_connections, self.database.max_connections
                ),
            });
        }

        // Server port (0 means "pick a random OS port" which is valid in tests
        // but not in production; we only reject it if a non-zero port is expected)
        // Note: port = 0 is allowed by design (OS-assigned). No check added here.

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
                },
                AuthProvider::Auth0 | AuthProvider::Clerk => {
                    if self.auth.domain.is_none() {
                        return Err(FraiseQLError::Configuration {
                            message: format!(
                                "auth.domain is required when using {:?} provider",
                                self.auth.provider
                            ),
                        });
                    }
                },
                AuthProvider::Webhook | AuthProvider::None => {},
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
/// Supports both `${VAR}` and `$VAR` syntax. The `${VAR}` form is matched
/// first (higher priority) so that `${FOO}BAR` expands the braced form only.
#[allow(clippy::expect_used)] // Reason: regex patterns are compile-time constants guaranteed to be valid
fn expand_env_vars(content: &str) -> String {
    use std::sync::LazyLock;

    // Matches ${VAR} (braced form)
    static BRACED_REGEX: LazyLock<regex::Regex> = LazyLock::new(|| {
        regex::Regex::new(r"\$\{([A-Za-z_][A-Za-z0-9_]*)\}").expect("braced env var regex is valid")
    });

    // Matches $VAR (bare form). Applied after the braced pass so any ${VAR}
    // patterns have already been resolved and won't be double-matched.
    static BARE_REGEX: LazyLock<regex::Regex> = LazyLock::new(|| {
        regex::Regex::new(r"\$([A-Za-z_][A-Za-z0-9_]*)").expect("bare env var regex is valid")
    });

    let expand = |input: &str, re: &regex::Regex| -> String {
        let mut result = input.to_string();
        // Collect matches before replacing to avoid offset issues
        let replacements: Vec<(String, String)> = re
            .captures_iter(input)
            .filter_map(|cap| {
                let full = cap.get(0)?.as_str().to_string();
                let var_name = cap.get(1)?.as_str();
                let value = std::env::var(var_name).ok()?;
                Some((full, value))
            })
            .collect();
        for (pattern, value) in replacements {
            result = result.replace(&pattern, &value);
        }
        result
    };

    // Expand braced form first, then bare form on the result
    let after_braced = expand(content, &BRACED_REGEX);
    expand(&after_braced, &BARE_REGEX)
}

/// Configuration builder.
#[must_use = "call .build() to construct the final value"]
#[derive(Debug, Default)]
pub struct ConfigBuilder {
    config: FraiseQLConfig,
}

impl ConfigBuilder {
    /// Set the database URL.
    pub fn database_url(mut self, url: &str) -> Self {
        self.config.database.url = url.to_string();
        self.config.database_url = url.to_string();
        self
    }

    /// Set the server host.
    pub fn host(mut self, host: &str) -> Self {
        self.config.server.host = host.to_string();
        self.config.host = host.to_string();
        self
    }

    /// Set the server port.
    pub const fn port(mut self, port: u16) -> Self {
        self.config.server.port = port;
        self.config.port = port;
        self
    }

    /// Set maximum database connections.
    pub const fn max_connections(mut self, n: u32) -> Self {
        self.config.database.max_connections = n;
        self.config.max_connections = n;
        self
    }

    /// Set query timeout.
    pub const fn query_timeout(mut self, secs: u64) -> Self {
        self.config.database.query_timeout_secs = secs;
        self.config.query_timeout_secs = secs;
        self
    }

    /// Set CORS configuration.
    pub fn cors(mut self, cors: CorsConfig) -> Self {
        self.config.cors = cors;
        self
    }

    /// Set auth configuration.
    pub fn auth(mut self, auth: AuthConfig) -> Self {
        self.config.auth = auth;
        self
    }

    /// Set rate limit configuration.
    pub fn rate_limit(mut self, rate_limit: RateLimitConfig) -> Self {
        self.config.rate_limit = rate_limit;
        self
    }

    /// Set cache configuration.
    pub const fn cache(mut self, cache: CacheConfig) -> Self {
        self.config.cache = cache;
        self
    }

    /// Set collation configuration.
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
    #![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

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
        assert!(
            matches!(result, Err(FraiseQLError::Configuration { .. })),
            "expected Configuration error when database URL is absent, got: {result:?}"
        );
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
        temp_env::with_vars(
            [
                ("TEST_DB_URL", Some("postgresql://user:pass@host/db")),
                ("TEST_JWT_SECRET", Some("super-secret")),
            ],
            || {
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
            },
        );
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
        assert!(
            matches!(validation, Err(FraiseQLError::Configuration { .. })),
            "expected Configuration error for missing jwt_secret, got: {validation:?}"
        );
        assert!(validation.unwrap_err().to_string().contains("jwt_secret is required"));
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
        assert!(
            matches!(validation, Err(FraiseQLError::Configuration { .. })),
            "expected Configuration error for missing auth0 domain, got: {validation:?}"
        );
        assert!(validation.unwrap_err().to_string().contains("domain is required"));
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
    fn test_mutation_timing_default_disabled() {
        let config = FraiseQLConfig::default();
        assert!(!config.database.mutation_timing.enabled);
        assert_eq!(config.database.mutation_timing.variable_name, "fraiseql.started_at");
    }

    #[test]
    fn test_mutation_timing_from_toml() {
        let toml = r#"
[database]
url = "postgresql://localhost/test"

[database.mutation_timing]
enabled = true
variable_name = "app.started_at"
"#;
        let config = FraiseQLConfig::from_toml(toml).unwrap();
        assert!(config.database.mutation_timing.enabled);
        assert_eq!(config.database.mutation_timing.variable_name, "app.started_at");
    }

    #[test]
    fn test_mutation_timing_from_toml_default_variable() {
        let toml = r#"
[database]
url = "postgresql://localhost/test"

[database.mutation_timing]
enabled = true
"#;
        let config = FraiseQLConfig::from_toml(toml).unwrap();
        assert!(config.database.mutation_timing.enabled);
        assert_eq!(config.database.mutation_timing.variable_name, "fraiseql.started_at");
    }

    #[test]
    fn test_mutation_timing_absent_uses_defaults() {
        let toml = r#"
[database]
url = "postgresql://localhost/test"
"#;
        let config = FraiseQLConfig::from_toml(toml).unwrap();
        assert!(!config.database.mutation_timing.enabled);
        assert_eq!(config.database.mutation_timing.variable_name, "fraiseql.started_at");
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
        let collation = CollationConfig {
            enabled: false,
            fallback_locale: "de-DE".to_string(),
            ..Default::default()
        };

        let config = FraiseQLConfig::builder()
            .database_url("postgresql://localhost/test")
            .collation(collation)
            .build()
            .unwrap();

        assert!(!config.collation.enabled);
        assert_eq!(config.collation.fallback_locale, "de-DE");
    }
}
