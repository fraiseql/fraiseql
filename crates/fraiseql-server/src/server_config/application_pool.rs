//! `[application_pool]`: the application database pool the binary opens.
//!
//! The RBAC backend, SCIM, tenant admin tokens, API keys, async operations, the `postgres`
//! session-state backend, sources, inbound webhooks and mail, and function dispatch tracking
//! all run against this pool (see [`crate::ApplicationPool`]). The binary used to open a pool
//! only in builds with the `observers` feature, and every one of those capabilities was
//! absent from the default build.
//!
//! The pool keeps no idle connection: `min_connections` is 0, so a deployment that configures
//! none of those capabilities holds no connection for it. It opens connections on first use,
//! so it adds no way for the boot to fail that the adapter's pool does not already have.

use std::time::Duration;

use serde::{Deserialize, Serialize};
use sqlx::postgres::{PgConnectOptions, PgPool, PgPoolOptions, PgSslMode};

use super::ServerConfig;

/// Sizing of the application pool.
///
/// ```toml
/// [application_pool]
/// max_connections = 5
/// acquire_timeout_secs = 10
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct ApplicationPoolConfig {
    /// Most connections the pool opens at once (default: 5).
    pub max_connections: u32,

    /// How long a caller waits for a connection, in seconds (default: 10).
    pub acquire_timeout_secs: u64,
}

impl ApplicationPoolConfig {
    /// The default `acquire_timeout_secs`.
    pub const DEFAULT_ACQUIRE_TIMEOUT_SECS: u64 = 10;
    /// The default `max_connections`.
    pub const DEFAULT_MAX_CONNECTIONS: u32 = 5;

    /// Validate the section.
    ///
    /// # Errors
    ///
    /// Returns a message naming the field when either value is zero.
    pub fn validate(&self) -> Result<(), String> {
        if self.max_connections == 0 {
            return Err("[application_pool] max_connections must be at least 1".to_string());
        }
        if self.acquire_timeout_secs == 0 {
            return Err("[application_pool] acquire_timeout_secs must be at least 1".to_string());
        }
        Ok(())
    }
}

impl Default for ApplicationPoolConfig {
    fn default() -> Self {
        Self {
            max_connections:      Self::DEFAULT_MAX_CONNECTIONS,
            acquire_timeout_secs: Self::DEFAULT_ACQUIRE_TIMEOUT_SECS,
        }
    }
}

/// The connection options `database_url` and `[database_tls]` describe, for a `sqlx` pool.
///
/// `[database_tls]` applies to every pool the server opens on its database, not only the
/// adapter's: an unset mode leaves `?sslmode=` in the URL to decide, as the adapter does.
///
/// # Errors
///
/// Returns a message when `[database_tls]` is invalid or `database_url` is not a PostgreSQL
/// connection URL. The parse error is not quoted: it can carry the URL.
pub fn connect_options(config: &ServerConfig) -> Result<PgConnectOptions, String> {
    use fraiseql_core::db::postgres::PostgresSslMode;

    let tls = config.postgres_tls()?;
    let mut options: PgConnectOptions = config
        .database_url
        .parse()
        .map_err(|_| "database_url is not a PostgreSQL connection URL".to_string())?;
    if let Some(mode) = tls.mode {
        options = options.ssl_mode(match mode {
            PostgresSslMode::Disable => PgSslMode::Disable,
            PostgresSslMode::Prefer => PgSslMode::Prefer,
            PostgresSslMode::Require => PgSslMode::Require,
            PostgresSslMode::VerifyFull => PgSslMode::VerifyFull,
        });
    }
    if let Some(ref path) = tls.ca_bundle_path {
        options = options.ssl_root_cert(path);
    }
    Ok(options)
}

/// Open the application pool `config` describes.
///
/// Connects on first use: building it runs no database I/O.
///
/// # Errors
///
/// See [`connect_options`].
pub fn open_application_pool(config: &ServerConfig) -> Result<PgPool, String> {
    let sizing = &config.application_pool;
    let options = connect_options(config)?;
    tracing::info!(
        max_connections = sizing.max_connections,
        acquire_timeout_secs = sizing.acquire_timeout_secs,
        "Application PostgreSQL pool ready (connects on first use, keeps none idle)"
    );
    Ok(PgPoolOptions::new()
        .min_connections(0)
        .max_connections(sizing.max_connections)
        .acquire_timeout(Duration::from_secs(sizing.acquire_timeout_secs))
        .connect_lazy_with(options))
}

#[cfg(test)]
#[path = "application_pool_tests.rs"]
mod tests;
