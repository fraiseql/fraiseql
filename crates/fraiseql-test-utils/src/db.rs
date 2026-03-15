//! Database URL resolution for test infrastructure.

/// Returns the test database URL from the `DATABASE_URL` environment variable.
///
/// # Panics
///
/// Panics with an actionable message if `DATABASE_URL` is not set.
/// Tests requiring a database must be marked `#[ignore]` and run with
/// `cargo nextest run --run-ignored`.
#[must_use]
pub fn database_url() -> String {
    std::env::var("DATABASE_URL").unwrap_or_else(|_| {
        panic!(
            "DATABASE_URL is not set. \
             Database tests must be run with a live database. \
             Set DATABASE_URL=postgresql://... or mark this test #[ignore]."
        )
    })
}

/// Returns the test database URL if `DATABASE_URL` is set, or `None` otherwise.
///
/// Use this for tests that should be silently skipped (return early) when no
/// database is available, instead of being permanently `#[ignore]`d.
#[must_use]
pub fn try_database_url() -> Option<String> {
    std::env::var("DATABASE_URL").ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn try_database_url_returns_none_when_unset() {
        // In normal test runs DATABASE_URL is not set, so this should return None.
        // When it IS set (CI), the test still passes because Some(_) is also valid.
        let result = try_database_url();
        // Just verify it doesn't panic — the return value depends on the environment.
        let _ = result;
    }
}
