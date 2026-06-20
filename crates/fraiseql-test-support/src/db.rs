//! Canonical database-URL resolution.
//!
//! This is the one place the database test-URL policy lives — `DATABASE_URL` for
//! PostgreSQL, `MYSQL_URL` for MySQL, `SQLSERVER_URL` for SQL Server. `fraiseql-test-utils`
//! re-exports these so its existing callers keep working without a second copy. Each
//! `*_url()` panics loudly when unset; each `try_*_url()` returns `None` for self-skipping
//! tests.

#![allow(clippy::panic)] // Reason: test infrastructure — panics with an actionable message are acceptable

use crate::services::env_url;

/// Returns the PostgreSQL test URL from the `DATABASE_URL` environment variable.
///
/// # Panics
///
/// Panics with an actionable message if `DATABASE_URL` is not set. Tests
/// requiring a database must be run with a live database (via Dagger or an
/// exported URL) or marked `#[ignore]`.
#[must_use]
pub fn database_url() -> String {
    resolve_or_panic("DATABASE_URL", "postgresql://...", try_database_url())
}

/// Returns the PostgreSQL test URL if `DATABASE_URL` is set, or `None` otherwise.
///
/// Use this for tests that should be silently skipped (return early) when no
/// database is available, instead of being permanently `#[ignore]`d.
#[must_use]
pub fn try_database_url() -> Option<String> {
    env_url("DATABASE_URL")
}

/// Returns the MySQL test URL from the `MYSQL_URL` environment variable.
///
/// # Panics
///
/// Panics with an actionable message if `MYSQL_URL` is not set (see [`database_url`]).
#[must_use]
pub fn mysql_url() -> String {
    resolve_or_panic("MYSQL_URL", "mysql://...", try_mysql_url())
}

/// Returns the MySQL test URL if `MYSQL_URL` is set, or `None` otherwise.
#[must_use]
pub fn try_mysql_url() -> Option<String> {
    env_url("MYSQL_URL")
}

/// Returns the SQL Server test connection string from the `SQLSERVER_URL` environment
/// variable (ADO form, e.g. `server=host,1433;database=...;user=...;password=...`).
///
/// # Panics
///
/// Panics with an actionable message if `SQLSERVER_URL` is not set (see [`database_url`]).
#[must_use]
pub fn sqlserver_url() -> String {
    resolve_or_panic("SQLSERVER_URL", "server=host,1433;database=...", try_sqlserver_url())
}

/// Returns the SQL Server test connection string if `SQLSERVER_URL` is set, or `None`.
#[must_use]
pub fn try_sqlserver_url() -> Option<String> {
    env_url("SQLSERVER_URL")
}

/// Resolve a database URL or panic loudly. Split out so the loud-failure contract is
/// unit-testable without manipulating process env. A swallowed or silently-defaulted URL
/// here would let every DB-backed test skip when CI fails to inject the URL — a false-green
/// meta-risk larger than most single findings.
#[must_use]
fn resolve_or_panic(var: &str, example: &str, url: Option<String>) -> String {
    url.unwrap_or_else(|| {
        panic!(
            "{var} is not set. Database tests must run against a live database. \
             Set {var}={example} (e.g. via `dagger call test-integration`), \
             or mark this test #[ignore]."
        )
    })
}

#[cfg(test)]
mod tests;
