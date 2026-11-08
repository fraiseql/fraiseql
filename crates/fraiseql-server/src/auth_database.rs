//! `[auth.database]`: the pool first-party auth runs on, as a role of its own.
//!
//! Every auth table lives in schema `auth`, created and owned by the role this pool
//! authenticates as. The engine's pool never reads them, so the engine's role needs no
//! grant on `auth`, and auth's role needs none on the application's schemas.
//!
//! The compiled schema names the environment variable that holds the URL, never the URL:
//! it carries the auth role's password. Nothing here logs or returns the URL.

use std::time::Duration;

use fraiseql_core::schema::AuthDatabaseConfig;
use sqlx::postgres::{PgConnectOptions, PgPool, PgPoolOptions};
use tracing::info;

use crate::ServerError;

/// Build the auth pool when an auth surface needs it.
///
/// `surfaces` names every first-party auth surface this server mounts. Each one stores
/// accounts, credentials or sessions, so it needs `[auth.database]`; and
/// `[auth.database]` with no surface to serve is a setting nothing reads.
///
/// # Errors
///
/// [`ServerError::ConfigError`] when a surface has no `[auth.database]`, when
/// `[auth.database]` has no surface, when the section is malformed, when its variable is
/// unset, empty or not a PostgreSQL URL.
///
/// The pool connects on first use, as every store does: construction runs no database
/// I/O. A database that refuses the auth role fails the boot at provisioning, before the
/// server listens.
pub fn build_auth_pool(
    database: Option<&AuthDatabaseConfig>,
    surfaces: &[&str],
) -> Result<Option<PgPool>, ServerError> {
    let database = match (database, surfaces.is_empty()) {
        (None, true) => return Ok(None),
        (None, false) => {
            return Err(ServerError::ConfigError(format!(
                "{} store accounts and sessions in schema `auth`, which first-party auth \
                 reaches through its own role: add [auth.database] with url_env naming the \
                 environment variable that holds that role's connection URL",
                surfaces.join(", ")
            )));
        },
        (Some(_), true) => {
            return Err(ServerError::ConfigError(
                "[auth.database] is set, but no first-party auth surface is configured \
                 ([auth.local], [auth.social], [saml], [scim]): nothing would use it"
                    .to_string(),
            ));
        },
        (Some(database), false) => database,
    };
    database.check().map_err(ServerError::ConfigError)?;

    let name = database.url_env.trim();
    let url = match std::env::var(name) {
        Ok(url) if url.trim().is_empty() => {
            return Err(ServerError::ConfigError(format!(
                "[auth.database] url_env names `{name}`, which is empty"
            )));
        },
        Ok(url) => url,
        Err(_) => {
            return Err(ServerError::ConfigError(format!(
                "[auth.database] url_env names `{name}`, which is not set"
            )));
        },
    };
    // The parse error is dropped on purpose: it can quote the URL.
    let options: PgConnectOptions = url.parse().map_err(|_| {
        ServerError::ConfigError(format!(
            "[auth.database] the value of `{name}` is not a PostgreSQL connection URL"
        ))
    })?;

    let max = database.pool_max.unwrap_or(AuthDatabaseConfig::DEFAULT_POOL_MAX);
    let timeout = database
        .acquire_timeout_secs
        .unwrap_or(AuthDatabaseConfig::DEFAULT_ACQUIRE_TIMEOUT_SECS);
    let pool = PgPoolOptions::new()
        .min_connections(database.pool_min.unwrap_or(0))
        .max_connections(max)
        .acquire_timeout(Duration::from_secs(timeout))
        .connect_lazy_with(options);
    info!(url_env = name, pool_max = max, "First-party auth runs on its own pool");
    Ok(Some(pool))
}
