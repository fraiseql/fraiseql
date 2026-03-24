//! Database URL resolution and pool creation for test infrastructure.

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

/// Create a PostgreSQL connection pool for integration tests.
///
/// Connects to the database specified by the `DATABASE_URL` environment variable
/// with a maximum of 5 connections.
///
/// # Panics
///
/// Panics if `DATABASE_URL` is not set or if the connection fails.
#[cfg(feature = "postgres")]
pub async fn create_test_pool() -> sqlx::PgPool {
    sqlx::postgres::PgPoolOptions::new()
        .max_connections(5)
        .connect(&database_url())
        .await
        .expect("Failed to connect to test database")
}

#[cfg(test)]
mod tests {
    // database_url() panics without DATABASE_URL set, so no unit test for it here.
    // Integration callers are expected to set the env var or use #[ignore].
}
