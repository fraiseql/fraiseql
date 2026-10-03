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
use deadpool_postgres::{Config, ManagerConfig, Pool, PoolConfig, RecyclingMethod, Runtime};
use tokio_postgres::NoTls;

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
/// `purpose` completes "failed to create a PostgreSQL connection pool for …".
/// Connection failures surface lazily, on the first query.
///
/// # Errors
///
/// Returns the [`require_postgres`] error, or one naming `purpose` when the pool
/// cannot be created.
pub(crate) fn postgres_pool(db_url: &str, purpose: &str) -> Result<Pool> {
    require_postgres(db_url)?;
    let mut cfg = Config::new();
    cfg.url = Some(db_url.to_string());
    cfg.manager = Some(ManagerConfig {
        recycling_method: RecyclingMethod::Fast,
    });
    cfg.pool = Some(PoolConfig::new(CLI_POOL_SIZE));
    cfg.create_pool(Some(Runtime::Tokio1), NoTls)
        .with_context(|| format!("failed to create a PostgreSQL connection pool for {purpose}"))
}

#[cfg(test)]
mod tests;
