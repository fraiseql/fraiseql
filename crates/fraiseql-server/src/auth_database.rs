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

/// Where releases before 2.19.1 kept first-party auth's tables, before schema `auth`, as
/// (schema, table): the one place this crate names that location on purpose.
const EARLIER_AUTH_TABLES: [(&str, &str); 15] = [
    ("_system", "sessions"),
    ("core", "tb_auth_identity"),
    ("core", "tb_email_verification_token"),
    ("core", "tb_mfa_challenge"),
    ("core", "tb_mfa_enrollment"),
    ("core", "tb_otp_code"),
    ("core", "tb_otp_send_budget"),
    ("core", "tb_password_credential"),
    ("core", "tb_password_reset_token"),
    ("core", "tb_saml_idp"),
    ("core", "tb_saml_replay"),
    ("core", "tb_scim_group"),
    ("core", "tb_scim_group_member"),
    ("core", "tb_scim_token"),
    ("core", "tb_user"),
];

/// Refuse to boot first-party auth over a database a release before 2.19.1 provisioned.
///
/// Those releases kept auth's tables outside schema `auth`. This release reads none of
/// them, so booting would serve sign-ins over empty tables in `auth`.
///
/// # Errors
///
/// [`ServerError::ConfigError`] naming every such table found, or
/// [`ServerError::Database`] when the catalog cannot be read.
pub async fn refuse_earlier_auth_tables(pool: &PgPool) -> Result<(), ServerError> {
    let (schemas, tables): (Vec<&str>, Vec<&str>) = EARLIER_AUTH_TABLES.into_iter().unzip();
    // The catalog, not `to_regclass`: auth's role has no USAGE on those schemas, and
    // `pg_class` is readable without it.
    let found: Vec<String> = sqlx::query_scalar(
        "SELECT e.schema || '.' || e.name \
         FROM unnest($1::text[], $2::text[]) AS e(schema, name) \
         JOIN pg_namespace n ON n.nspname = e.schema \
         JOIN pg_class c ON c.relnamespace = n.oid AND c.relname = e.name \
         ORDER BY 1",
    )
    .bind(schemas)
    .bind(tables)
    .fetch_all(pool)
    .await
    .map_err(|e| ServerError::Database(format!("could not read the catalog: {e}")))?;
    if found.is_empty() {
        return Ok(());
    }
    Err(ServerError::ConfigError(format!(
        "this database holds first-party auth tables from a release before 2.19.1, which \
         kept them outside schema `auth`: {}. This release does not read them, and does \
         not start over them.",
        found.join(", ")
    )))
}
