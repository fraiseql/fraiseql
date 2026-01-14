//! Server configuration.

use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use std::path::PathBuf;

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

    /// Database connection pool minimum size.
    #[serde(default = "default_pool_min_size")]
    pub pool_min_size: usize,

    /// Database connection pool maximum size.
    #[serde(default = "default_pool_max_size")]
    pub pool_max_size: usize,

    /// Database connection pool timeout in seconds.
    #[serde(default = "default_pool_timeout")]
    pub pool_timeout_secs: u64,
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
            pool_min_size: default_pool_min_size(),
            pool_max_size: default_pool_max_size(),
            pool_timeout_secs: default_pool_timeout(),
        }
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
        assert!(config.cors_enabled);
        assert!(config.compression_enabled);
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
}
