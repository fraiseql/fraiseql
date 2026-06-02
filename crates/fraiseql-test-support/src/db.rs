//! Canonical database-URL resolution.
//!
//! This is the one place the `DATABASE_URL` policy lives; `fraiseql-test-utils`
//! re-exports these so its existing callers keep working without a second copy.

#![allow(clippy::panic)] // Reason: test infrastructure — panics with an actionable message are acceptable

use crate::services::env_url;

/// Returns the test database URL from the `DATABASE_URL` environment variable.
///
/// # Panics
///
/// Panics with an actionable message if `DATABASE_URL` is not set. Tests
/// requiring a database must be run with a live database (via Dagger or an
/// exported URL) or marked `#[ignore]`.
#[must_use]
pub fn database_url() -> String {
    try_database_url().unwrap_or_else(|| {
        panic!(
            "DATABASE_URL is not set. Database tests must run against a live database. \
             Set DATABASE_URL=postgresql://... (e.g. via `dagger call test-integration`), \
             or mark this test #[ignore]."
        )
    })
}

/// Returns the test database URL if `DATABASE_URL` is set, or `None` otherwise.
///
/// Use this for tests that should be silently skipped (return early) when no
/// database is available, instead of being permanently `#[ignore]`d.
#[must_use]
pub fn try_database_url() -> Option<String> {
    env_url("DATABASE_URL")
}
