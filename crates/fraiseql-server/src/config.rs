//! Server configuration.

use serde::{Deserialize, Serialize};
use std::net::SocketAddr;

/// Server configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerConfig {
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
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
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
        }
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = ServerConfig::default();
        assert_eq!(config.graphql_path, "/graphql");
        assert_eq!(config.health_path, "/health");
        assert!(config.cors_enabled);
        assert!(config.compression_enabled);
    }
}
