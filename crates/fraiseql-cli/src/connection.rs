//! The one PostgreSQL connection-string rule for every CLI command.
//!
//! Seven call sites each decided "is this a PostgreSQL URL" with their own prefix
//! test — `starts_with("postgres")`, or `postgres://` / `postgresql://` — and so
//! disagreed about the libpq `key=value` form (`host=db dbname=app`) that
//! PostgreSQL itself, `tokio-postgres` and the server all accept. The disagreement
//! was not cosmetic: `compile --database` ran the drift check over a `key=value`
//! string and then silently skipped the mutation-contract check, which asked the
//! prefix question again (#1403).
//!
//! The rule is now: refuse an engine whose support was removed, then accept
//! whatever `tokio_postgres::Config` parses. Error messages never echo the
//! connection string, which may carry a password.

use anyhow::{Context, Result};
use deadpool_postgres::Pool;
use fraiseql_db::postgres::{
    PoolPrewarmConfig, PostgresAdapter, PostgresTlsConfig, VectorScanConfig,
};

use crate::schema::database_validator::refuse_removed_engine_url;

/// Connections a CLI pool opens: the CLI reads the catalogue, it does not serve.
const CLI_POOL_SIZE: usize = 2;

/// Check that `db_url` is a PostgreSQL connection string, in URL or libpq form.
///
/// # Errors
///
/// Returns an error naming the PostgreSQL-only rule for a removed engine's URL,
/// and one naming the two accepted forms for anything `tokio-postgres` cannot
/// parse.
pub fn require_postgres(db_url: &str) -> Result<()> {
    refuse_removed_engine_url(db_url)?;
    db_url.parse::<tokio_postgres::Config>().map_err(|e| {
        anyhow::anyhow!(
            "Not a PostgreSQL connection string ({e}). Pass a postgresql://… URL or a libpq \
             `key=value` string such as `host=localhost dbname=app user=app`."
        )
    })?;
    Ok(())
}

/// A small connection pool for a CLI read, after [`require_postgres`].
///
/// Built by `fraiseql-db`'s own pool constructor, so the CLI and the server share one
/// TLS policy (#1429): the URL's `sslmode` applies when `tls` sets no mode, a mode in
/// `tls` (from `[database] ssl_mode`) overrides it, and the connector negotiates TLS
/// rather than refusing it. The first connection is opened here, and the server is
/// refused below the supported PostgreSQL floor before the command reads anything.
///
/// `purpose` completes "failed to connect to PostgreSQL for …".
///
/// # Errors
///
/// Returns the [`require_postgres`] error, one naming `purpose` when the server cannot
/// be reached or the TLS settings cannot be honoured, and the floor refusal for a server
/// older than PostgreSQL 18.
pub async fn postgres_pool(db_url: &str, purpose: &str, tls: &PostgresTlsConfig) -> Result<Pool> {
    Ok(postgres_adapter(db_url, purpose, tls).await?.pool().clone())
}

/// The adapter behind [`postgres_pool`], for a command that executes through the
/// runtime rather than reading the catalogue (`doctor --runtime`).
///
/// # Errors
///
/// As [`postgres_pool`].
pub async fn postgres_adapter(
    db_url: &str,
    purpose: &str,
    tls: &PostgresTlsConfig,
) -> Result<PostgresAdapter> {
    require_postgres(db_url)?;
    PostgresAdapter::with_pool_config(
        db_url,
        PoolPrewarmConfig {
            min_size:            0,
            max_size:            CLI_POOL_SIZE,
            timeout_secs:        None,
            search_path:         None,
            tls:                 tls.clone(),
            read_replicas:       None,
            max_streaming_reads: None,
            vector_scan:         VectorScanConfig::default(),
        },
    )
    .await
    .map_err(anyhow::Error::new)
    .with_context(|| {
        format!(
            "failed to connect to PostgreSQL for {purpose} (ssl_mode = {})",
            tls.effective_mode()
        )
    })
}

#[cfg(test)]
mod tests;
