//! TLS/SSL server configuration and enforcement.
//!
//! This module handles:
//! - Loading and validating TLS certificates and keys
//! - Building TLS acceptance profiles for servers
//! - Configuring mTLS (client certificate requirements)
//! - Database connection TLS settings
//! - Per-connection TLS enforcement using the `TlsEnforcer`

use std::{fmt::Write as _, path::Path, sync::Arc};

use fraiseql_core::security::{TlsConfig, TlsEnforcer, TlsVersion};
use rustls::{ServerConfig, pki_types::CertificateDer};
use rustls_pemfile::Item;
use tracing::info;

use crate::{
    Result, ServerError,
    server_config::{DatabaseTlsConfig, TlsServerConfig},
};

/// TLS server setup and enforcement.
pub struct TlsSetup {
    /// TLS enforcer for validating connections.
    enforcer: TlsEnforcer,

    /// Server TLS configuration.
    config: Option<TlsServerConfig>,

    /// Database TLS configuration.
    db_config: Option<DatabaseTlsConfig>,
}

impl TlsSetup {
    /// Create new TLS setup from server configuration.
    ///
    /// # Errors
    ///
    /// Returns error if:
    /// - TLS is enabled but certificate/key files cannot be read
    /// - TLS configuration is invalid
    pub fn new(
        tls_config: Option<TlsServerConfig>,
        db_tls_config: Option<DatabaseTlsConfig>,
    ) -> Result<Self> {
        // Create the enforcer based on configuration
        let enforcer = if let Some(ref tls) = tls_config {
            if tls.enabled {
                Self::create_enforcer(tls)?
            } else {
                TlsEnforcer::permissive()
            }
        } else {
            TlsEnforcer::permissive()
        };

        Ok(Self {
            enforcer,
            config: tls_config,
            db_config: db_tls_config,
        })
    }

    /// Create a TLS enforcer from configuration.
    fn create_enforcer(config: &TlsServerConfig) -> Result<TlsEnforcer> {
        // Parse minimum TLS version
        let min_version = match config.min_version.as_str() {
            "1.2" => TlsVersion::V1_2,
            "1.3" => TlsVersion::V1_3,
            other => {
                return Err(ServerError::ConfigError(format!(
                    "Invalid TLS minimum version: {}",
                    other
                )));
            },
        };

        // Create TLS configuration
        let tls_config = TlsConfig {
            tls_required: true,
            mtls_required: config.require_client_cert,
            min_version,
        };

        info!(
            tls_enabled = true,
            require_mtls = config.require_client_cert,
            min_version = %min_version,
            "TLS configuration loaded"
        );

        Ok(TlsEnforcer::from_config(tls_config))
    }

    /// Get the TLS enforcer.
    #[must_use]
    pub const fn enforcer(&self) -> &TlsEnforcer {
        &self.enforcer
    }

    /// Get the server TLS configuration.
    #[must_use]
    pub const fn config(&self) -> &Option<TlsServerConfig> {
        &self.config
    }

    /// Get the database TLS configuration.
    #[must_use]
    pub const fn db_config(&self) -> &Option<DatabaseTlsConfig> {
        &self.db_config
    }

    /// Check if TLS is enabled for server.
    #[must_use]
    pub fn is_tls_enabled(&self) -> bool {
        self.config.as_ref().is_some_and(|c| c.enabled)
    }

    /// Check if mTLS is required.
    #[must_use]
    pub fn is_mtls_required(&self) -> bool {
        self.config.as_ref().is_some_and(|c| c.enabled && c.require_client_cert)
    }

    /// Get the certificate path.
    #[must_use]
    pub fn cert_path(&self) -> Option<&Path> {
        self.config.as_ref().map(|c| c.cert_path.as_path())
    }

    /// Get the key path.
    #[must_use]
    pub fn key_path(&self) -> Option<&Path> {
        self.config.as_ref().map(|c| c.key_path.as_path())
    }

    /// Get the client CA path (for mTLS).
    #[must_use]
    pub fn client_ca_path(&self) -> Option<&Path> {
        self.config
            .as_ref()
            .and_then(|c| c.client_ca_path.as_ref())
            .map(|p| p.as_path())
    }

    /// Get PostgreSQL SSL mode for database connections.
    #[must_use]
    pub fn postgres_ssl_mode(&self) -> &str {
        self.db_config.as_ref().map_or("prefer", |c| c.postgres_ssl_mode.as_str())
    }

    /// Check if Redis TLS is enabled.
    #[must_use]
    pub fn redis_ssl_enabled(&self) -> bool {
        self.db_config.as_ref().is_some_and(|c| c.redis_ssl)
    }

    /// Check if `ClickHouse` HTTPS is enabled.
    #[must_use]
    pub fn clickhouse_https_enabled(&self) -> bool {
        self.db_config.as_ref().is_some_and(|c| c.clickhouse_https)
    }

    /// Check if Elasticsearch HTTPS is enabled.
    #[must_use]
    pub fn elasticsearch_https_enabled(&self) -> bool {
        self.db_config.as_ref().is_some_and(|c| c.elasticsearch_https)
    }

    /// Check if certificate verification is enabled for databases.
    #[must_use]
    pub fn verify_certificates(&self) -> bool {
        self.db_config.as_ref().is_none_or(|c| c.verify_certificates)
    }

    /// Get the CA bundle path for verifying database certificates.
    #[must_use]
    pub fn ca_bundle_path(&self) -> Option<&Path> {
        self.db_config
            .as_ref()
            .and_then(|c| c.ca_bundle_path.as_ref())
            .map(|p| p.as_path())
    }

    /// Get database URL with TLS applied (for PostgreSQL).
    pub fn apply_postgres_tls(&self, db_url: &str) -> String {
        let mut url = db_url.to_string();

        // Parse SSL mode into URL parameter
        let ssl_mode = self.postgres_ssl_mode();
        if !ssl_mode.is_empty() && ssl_mode != "prefer" {
            // Add or update sslmode parameter
            if url.contains('?') {
                let _ = write!(url, "&sslmode={ssl_mode}");
            } else {
                let _ = write!(url, "?sslmode={ssl_mode}");
            }
        }

        url
    }

    /// Get Redis URL with TLS applied.
    pub fn apply_redis_tls(&self, redis_url: &str) -> String {
        if self.redis_ssl_enabled() {
            // Replace redis:// with rediss://
            redis_url.replace("redis://", "rediss://")
        } else {
            redis_url.to_string()
        }
    }

    /// Get `ClickHouse` URL with TLS applied.
    pub fn apply_clickhouse_tls(&self, ch_url: &str) -> String {
        if self.clickhouse_https_enabled() {
            // Replace http:// with https://
            ch_url.replace("http://", "https://")
        } else {
            ch_url.to_string()
        }
    }

    /// Get Elasticsearch URL with TLS applied.
    pub fn apply_elasticsearch_tls(&self, es_url: &str) -> String {
        if self.elasticsearch_https_enabled() {
            // Replace http:// with https://
            es_url.replace("http://", "https://")
        } else {
            es_url.to_string()
        }
    }

    /// Load certificates from PEM file.
    fn load_certificates(path: &Path) -> Result<Vec<CertificateDer<'static>>> {
        let cert_file = std::fs::File::open(path).map_err(|e| {
            ServerError::ConfigError(format!(
                "Failed to open certificate file {}: {}",
                path.display(),
                e
            ))
        })?;

        let mut reader = std::io::BufReader::new(cert_file);
        let mut certificates = Vec::new();

        loop {
            match rustls_pemfile::read_one(&mut reader).map_err(|e| {
                ServerError::ConfigError(format!("Failed to parse certificate: {}", e))
            })? {
                Some(Item::X509Certificate(cert)) => certificates.push(cert),
                Some(_) => {}, // Skip other items
                None => break,
            }
        }

        if certificates.is_empty() {
            return Err(ServerError::ConfigError(
                "No certificates found in certificate file".to_string(),
            ));
        }

        Ok(certificates)
    }

    /// Load private key from PEM file.
    fn load_private_key(path: &Path) -> Result<rustls::pki_types::PrivateKeyDer<'static>> {
        let key_file = std::fs::File::open(path).map_err(|e| {
            ServerError::ConfigError(format!("Failed to open key file {}: {}", path.display(), e))
        })?;

        let mut reader = std::io::BufReader::new(key_file);

        loop {
            match rustls_pemfile::read_one(&mut reader).map_err(|e| {
                ServerError::ConfigError(format!("Failed to parse private key: {}", e))
            })? {
                Some(Item::Pkcs8Key(key)) => return Ok(key.into()),
                Some(Item::Pkcs1Key(key)) => return Ok(key.into()),
                Some(Item::Sec1Key(key)) => return Ok(key.into()),
                Some(_) => {}, // Skip other items
                None => break,
            }
        }

        Err(ServerError::ConfigError("No private key found in key file".to_string()))
    }

    /// Create a rustls `ServerConfig` for TLS.
    ///
    /// # Errors
    ///
    /// Returns error if:
    /// - Certificate or key files cannot be read
    /// - Certificate or key format is invalid
    pub fn create_rustls_config(&self) -> Result<Arc<ServerConfig>> {
        let (cert_path, key_path) = match self.config.as_ref() {
            Some(c) if c.enabled => (&c.cert_path, &c.key_path),
            _ => return Err(ServerError::ConfigError("TLS not enabled".to_string())),
        };

        info!(
            cert_path = %cert_path.display(),
            key_path = %key_path.display(),
            "Loading TLS certificates"
        );

        let certs = Self::load_certificates(cert_path)?;
        let key = Self::load_private_key(key_path)?;

        let server_config = ServerConfig::builder()
            .with_no_client_auth()
            .with_single_cert(certs, key)
            .map_err(|e| ServerError::ConfigError(format!("Failed to build TLS config: {}", e)))?;

        Ok(Arc::new(server_config))
    }
}

