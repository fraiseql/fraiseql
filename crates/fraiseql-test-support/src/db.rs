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
    resolve_or_panic(try_database_url())
}

/// Resolve a database URL or panic loudly. Split out from [`database_url`] so the
/// loud-failure contract is unit-testable without manipulating process env. A
/// swallowed or silently-defaulted URL here would let every DB-backed test skip
/// when CI fails to inject `DATABASE_URL` — a false-green meta-risk larger than
/// most single findings.
#[must_use]
fn resolve_or_panic(url: Option<String>) -> String {
    url.unwrap_or_else(|| {
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

#[cfg(test)]
mod tests {
    use super::resolve_or_panic;

    /// The loud-failure contract: a missing DB URL must abort, not silently default.
    /// If this ever returns instead of panicking, DB-backed integration tests would
    /// pass vacuously whenever CI fails to inject `DATABASE_URL`.
    #[test]
    #[should_panic(expected = "DATABASE_URL is not set")]
    fn resolve_or_panic_is_loud_when_unset() {
        let _ = resolve_or_panic(None);
    }

    #[test]
    fn resolve_or_panic_returns_the_set_url() {
        assert_eq!(
            resolve_or_panic(Some("postgresql://localhost/test".to_string())),
            "postgresql://localhost/test"
        );
    }
}
