//! Boot-time detection of the unsupported server-side `[tls]` section.
//!
//! Server-side TLS termination is **not supported**: FraiseQL serves plaintext HTTP and
//! expects a reverse proxy (nginx, Caddy, a cloud load balancer, a service mesh) to
//! terminate TLS in front of it. The server **refuses to boot** if `[tls]` (server-side
//! TLS) is enabled — see `server/lifecycle.rs`. Previously a rustls `ServerConfig` was
//! built from `[tls]` and then silently discarded while the server kept serving plaintext
//! (M-tls-enforce), so the dead `TlsEnforcer` / `create_rustls_config` plumbing was removed.
//!
//! **Database** TLS no longer lives here. This module used to also carry
//! `apply_postgres_tls` / `apply_redis_tls` / `apply_clickhouse_tls` /
//! `apply_elasticsearch_tls` — URL-rewriting helpers with no production caller, whose
//! existence let `serve()` log "Database connection TLS configuration applied" over a pool
//! that had been built with `NoTls` several hundred lines earlier (#801). Transport
//! security is a property of the connection, so it is now decided where connections are
//! made: [`DatabaseTlsConfig::postgres_tls`] lowers the section onto
//! [`PostgresTlsConfig`](fraiseql_core::db::postgres::PostgresTlsConfig), which is a
//! required field of the pool configuration.
//!
//! [`DatabaseTlsConfig::postgres_tls`]: crate::server_config::DatabaseTlsConfig::postgres_tls

use crate::server_config::TlsServerConfig;

/// The server-side `[tls]` config, retained only so the boot path can detect (and refuse)
/// an enabled server-TLS configuration.
pub struct TlsSetup {
    /// Server TLS configuration (server-side TLS termination is unsupported; this is read
    /// only by [`is_tls_enabled`](Self::is_tls_enabled) for the boot-time refusal).
    config: Option<TlsServerConfig>,
}

impl TlsSetup {
    /// Create new TLS setup from server configuration.
    #[must_use]
    pub const fn new(tls_config: Option<TlsServerConfig>) -> Self {
        Self { config: tls_config }
    }

    /// Whether server-side `[tls]` is enabled in the configuration.
    ///
    /// Server-side TLS termination is unsupported, so the boot path uses this to refuse to
    /// start rather than serve plaintext under an enabled `[tls]` config (M-tls-enforce).
    #[must_use]
    pub fn is_tls_enabled(&self) -> bool {
        self.config.as_ref().is_some_and(|c| c.enabled)
    }
}
