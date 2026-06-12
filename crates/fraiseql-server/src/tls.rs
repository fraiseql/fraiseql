//! TLS/SSL configuration for **database connections**.
//!
//! Server-side TLS termination is **not supported**: FraiseQL serves plaintext HTTP and
//! expects a reverse proxy (nginx, Caddy, a cloud load balancer, a service mesh) to
//! terminate TLS in front of it. The server **refuses to boot** if `[tls]` (server-side
//! TLS) is enabled — see `server/lifecycle.rs`. Previously a rustls `ServerConfig` was
//! built from `[tls]` and then silently discarded while the server kept serving plaintext
//! (M-tls-enforce), so the dead `TlsEnforcer` / `create_rustls_config` plumbing was removed.
//!
//! This module retains only the **database** connection TLS settings (`postgres_ssl_mode`,
//! `redis_ssl`, etc.) and the URL-rewriting helpers that apply them, plus
//! [`TlsSetup::is_tls_enabled`] used by the boot-time refusal check.

use std::{fmt::Write as _, path::Path};

use crate::server_config::{DatabaseTlsConfig, TlsServerConfig};

/// Database connection TLS settings, plus the server-side `[tls]` config retained only so
/// the boot path can detect (and refuse) an enabled server-TLS configuration.
pub struct TlsSetup {
    /// Server TLS configuration (server-side TLS termination is unsupported; this is read
    /// only by [`is_tls_enabled`](Self::is_tls_enabled) for the boot-time refusal).
    config: Option<TlsServerConfig>,

    /// Database TLS configuration.
    db_config: Option<DatabaseTlsConfig>,
}

impl TlsSetup {
    /// Create new TLS setup from server configuration.
    #[must_use]
    pub const fn new(
        tls_config: Option<TlsServerConfig>,
        db_tls_config: Option<DatabaseTlsConfig>,
    ) -> Self {
        Self {
            config:    tls_config,
            db_config: db_tls_config,
        }
    }

    /// Get the database TLS configuration.
    #[must_use]
    pub const fn db_config(&self) -> &Option<DatabaseTlsConfig> {
        &self.db_config
    }

    /// Whether server-side `[tls]` is enabled in the configuration.
    ///
    /// Server-side TLS termination is unsupported, so the boot path uses this to refuse to
    /// start rather than serve plaintext under an enabled `[tls]` config (M-tls-enforce).
    #[must_use]
    pub fn is_tls_enabled(&self) -> bool {
        self.config.as_ref().is_some_and(|c| c.enabled)
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
    #[must_use]
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
    #[must_use]
    pub fn apply_redis_tls(&self, redis_url: &str) -> String {
        if self.redis_ssl_enabled() {
            // Replace redis:// with rediss://
            redis_url.replace("redis://", "rediss://")
        } else {
            redis_url.to_string()
        }
    }

    /// Get `ClickHouse` URL with TLS applied.
    #[must_use]
    pub fn apply_clickhouse_tls(&self, ch_url: &str) -> String {
        if self.clickhouse_https_enabled() {
            // Replace http:// with https://
            ch_url.replace("http://", "https://")
        } else {
            ch_url.to_string()
        }
    }

    /// Get Elasticsearch URL with TLS applied.
    #[must_use]
    pub fn apply_elasticsearch_tls(&self, es_url: &str) -> String {
        if self.elasticsearch_https_enabled() {
            // Replace http:// with https://
            es_url.replace("http://", "https://")
        } else {
            es_url.to_string()
        }
    }
}
