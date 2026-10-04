//! The PostgreSQL server floor.
//!
//! FraiseQL supports PostgreSQL 18 and newer, and only that. The engine's SQL, the
//! migrations it ships and the ones it generates may use any PostgreSQL 18 feature
//! without a version-detection fallback, so an older server does not degrade: it
//! fails, somewhere, with an error that names a function or a syntax rather than the
//! version. Every connection FraiseQL opens to serve or to read a catalogue
//! therefore asks the server its version first and refuses below the floor, naming
//! both versions and the upgrade path.

use fraiseql_error::FraiseQLError;

use super::pg_detail;
use crate::Result;

/// The lowest supported `server_version_num` (PostgreSQL 18.0).
pub const MINIMUM_SERVER_VERSION_NUM: i32 = 180_000;

/// Refuse a server whose `server_version_num` is below
/// [`MINIMUM_SERVER_VERSION_NUM`].
///
/// `server_version` is the human-readable version string, quoted in the message.
///
/// # Errors
///
/// Returns `FraiseQLError::Unsupported` naming the version found, the minimum and
/// the upgrade path.
pub fn check_server_version(server_version_num: i32, server_version: &str) -> Result<()> {
    if server_version_num >= MINIMUM_SERVER_VERSION_NUM {
        return Ok(());
    }
    Err(FraiseQLError::Unsupported {
        message: format!(
            "PostgreSQL {server_version} is not supported: FraiseQL requires PostgreSQL 18 or \
             newer. Upgrade the cluster with pg_upgrade, or dump it and restore into a \
             PostgreSQL 18 cluster."
        ),
    })
}

/// Ask the server behind `client` its version and refuse it below the floor.
///
/// # Errors
///
/// Returns `FraiseQLError::Database` when the version cannot be read, and the
/// [`check_server_version`] error when it is below the floor.
pub async fn require_supported_server(client: &tokio_postgres::Client) -> Result<()> {
    let row = client
        .query_one(
            "SELECT current_setting('server_version_num')::int, current_setting('server_version')",
            &[],
        )
        .await
        .map_err(|e| FraiseQLError::Database {
            message:   format!("Failed to read the PostgreSQL server version: {}", pg_detail(&e)),
            sql_state: e.code().map(|c| c.code().to_string()),
        })?;
    check_server_version(row.get(0), row.get(1))
}

#[cfg(test)]
mod tests;
