//! TLS configuration types for server and database connections.

use std::path::PathBuf;

use fraiseql_core::db::postgres::{PostgresSslMode, PostgresTlsConfig};
use serde::{Deserialize, Serialize};

use super::defaults::{default_tls_min_version, default_verify_certs};

/// GraphQL IDE/playground tool to use.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
#[non_exhaustive]
pub enum PlaygroundTool {
    /// `GraphiQL` - the classic GraphQL IDE.
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
///
/// Every field here reaches the PostgreSQL connection pool through
/// [`postgres_tls`](Self::postgres_tls). This section used to be parsed, validated
/// and logged as "applied" while the pool was built with `NoTls` regardless (#801);
/// the fields that could not be delivered were removed rather than left accepted.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatabaseTlsConfig {
    /// PostgreSQL SSL mode: `disable`, `prefer`, `require`, or `verify-full`.
    ///
    /// libpq's `allow` and `verify-ca` are rejected at load time with a message
    /// naming the mode to use instead — see [`PostgresSslMode`].
    ///
    /// Unset means "whatever the connection URL says", not "prefer": defaulting it
    /// to a concrete mode would override an operator's explicit
    /// `?sslmode=require` with a value they never wrote.
    #[serde(default)]
    pub postgres_ssl_mode: Option<String>,

    /// Verify the database server's certificate.
    ///
    /// Redundant with `postgres_ssl_mode`, and kept only to reject the
    /// contradiction: `verify_certificates = false` alongside
    /// `postgres_ssl_mode = "verify-full"` is an error rather than a silent
    /// downgrade of one or the other.
    #[serde(default = "default_verify_certs")]
    pub verify_certificates: bool,

    /// Path to a PEM bundle of certificate authorities used to verify the database
    /// server. When set, it replaces the platform trust store.
    #[serde(default)]
    pub ca_bundle_path: Option<PathBuf>,

    /// Removed. Retained solely so that a config still setting it is refused with a
    /// pointer instead of being silently ignored.
    ///
    /// Dropping the field outright would make `redis_ssl = true` an unknown key,
    /// and an unknown key in this struct is discarded without a word — the operator
    /// would go from "setting accepted and ignored" to "setting ignored", which is
    /// the same defect with less evidence.
    #[serde(default, skip_serializing)]
    pub redis_ssl: Option<bool>,

    /// Removed — see [`redis_ssl`](Self::redis_ssl).
    #[serde(default, skip_serializing)]
    pub clickhouse_https: Option<bool>,

    /// Removed — see [`redis_ssl`](Self::redis_ssl).
    #[serde(default, skip_serializing)]
    pub elasticsearch_https: Option<bool>,
}

impl DatabaseTlsConfig {
    /// Refuse the scheme-switch booleans that never reached a connection.
    ///
    /// `redis_ssl` / `clickhouse_https` / `elasticsearch_https` only ever rewrote a
    /// URL prefix, in a helper with no production caller. The URL already carries
    /// that information, so rather than reinstate a second, drift-prone way to say
    /// the same thing, the section names the one that works.
    fn reject_removed_scheme_switches(&self) -> std::result::Result<(), String> {
        for (field, value, replacement) in [
            ("redis_ssl", self.redis_ssl, "rediss://"),
            ("clickhouse_https", self.clickhouse_https, "https://"),
            ("elasticsearch_https", self.elasticsearch_https, "https://"),
        ] {
            if value.is_some() {
                return Err(format!(
                    "[database_tls] {field} has been removed: it only rewrote the URL scheme, \
                     and never reached any connection. Put the scheme in the URL directly \
                     ({replacement}…) — that is what the client library reads."
                ));
            }
        }
        Ok(())
    }

    /// Lower this section onto the pool's transport-security type.
    ///
    /// # Errors
    ///
    /// Returns a human-readable message when the ssl mode is unknown or
    /// unsupported, or when `verify_certificates` contradicts it.
    pub fn postgres_tls(&self) -> std::result::Result<PostgresTlsConfig, String> {
        self.reject_removed_scheme_switches()?;

        let mode: Option<PostgresSslMode> =
            self.postgres_ssl_mode.as_deref().map(str::parse).transpose().map_err(
                |e: fraiseql_error::FraiseQLError| format!("[database_tls] postgres_ssl_mode: {e}"),
            )?;

        if !self.verify_certificates && mode.is_some_and(PostgresSslMode::verifies_server) {
            return Err(
                "[database_tls] postgres_ssl_mode = \"verify-full\" verifies the database's \
                 certificate, but verify_certificates = false asks it not to. Set \
                 verify_certificates = true, or choose postgres_ssl_mode = \"require\" to \
                 encrypt without verifying."
                    .to_string(),
            );
        }

        let tls = PostgresTlsConfig {
            mode,
            ca_bundle_path: self.ca_bundle_path.clone(),
        };
        tls.validate().map_err(|e| format!("[database_tls]: {e}"))?;
        Ok(tls)
    }
}
