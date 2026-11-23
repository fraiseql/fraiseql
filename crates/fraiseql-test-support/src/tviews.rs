//! The `pg_tviews` database whose library is not preloaded.
//!
//! `TVIEWS_UNPRELOADED_DATABASE_URL` names it: built by
//! `docker/pg-tviews/unpreloaded-entrypoint.sh`, through `tools/pg-tviews-unpreloaded.sh`
//! locally and `pgTviewsUnpreloadedService` in `.dagger/main.go`.

#![allow(clippy::panic)] // Reason: test infrastructure — panics with an actionable message are acceptable

use crate::services::env_url;

/// The variable naming the database.
pub const TVIEWS_UNPRELOADED_URL_ENV: &str = "TVIEWS_UNPRELOADED_DATABASE_URL";

/// Set by the rigs that bind the database (the `pg_tviews` leg, `make test-pg-tviews`).
///
/// Where it is set, a test that finds no URL fails instead of skipping: the run promised
/// the database, so a skip there would read as a pass while asserting nothing.
pub const REQUIRE_TVIEWS_UNPRELOADED_ENV: &str = "FRAISEQL_TEST_REQUIRE_TVIEWS_UNPRELOADED";

/// The URL of the database whose `pg_tviews` is not preloaded, or `None` to skip.
///
/// # Panics
///
/// Panics when the URL is unset where [`REQUIRE_TVIEWS_UNPRELOADED_ENV`] is set.
#[must_use]
pub fn tviews_unpreloaded_database_url() -> Option<String> {
    demand(
        env_url(TVIEWS_UNPRELOADED_URL_ENV),
        env_url(REQUIRE_TVIEWS_UNPRELOADED_ENV).is_some(),
    )
}

/// `found`, unless it is `None` where `required`.
fn demand(found: Option<String>, required: bool) -> Option<String> {
    assert!(
        found.is_some() || !required,
        "{TVIEWS_UNPRELOADED_URL_ENV} is not set, but {REQUIRE_TVIEWS_UNPRELOADED_ENV} is: this \
         run binds the unpreloaded pg_tviews database (tools/pg-tviews-unpreloaded.sh up), so a \
         test that skipped here would read as a pass"
    );
    found
}

#[cfg(test)]
mod tests;
