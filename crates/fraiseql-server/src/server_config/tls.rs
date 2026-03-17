//! TLS configuration types for server and database connections.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use super::defaults::{
    default_clickhouse_https, default_elasticsearch_https, default_postgres_ssl_mode,
    default_redis_ssl, default_tls_min_version, default_verify_certs,
};

/// GraphQL IDE/playground tool to use.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
#[non_exhaustive]
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
