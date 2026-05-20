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
    #[must_use]
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
mod tests;
