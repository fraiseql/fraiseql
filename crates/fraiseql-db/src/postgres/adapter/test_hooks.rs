//! Adapter internals the PostgreSQL integration suite drives directly.
//!
//! The suite lives in `tests/` so that `cargo test --lib` can never reach a test that needs a
//! database (#1370), and an integration target sees only the public API. These few white-box
//! probes (connection-acquire retry, the raw read path, write pinning and the `NUMERIC`
//! decoder) are exposed here instead, compiled only with the test-only `test-postgres`
//! feature. Nothing returned names a driver type.

use super::{PostgresAdapter, ReadRouting, numeric::PgNumericText};
use crate::{Result, types::JsonbValue};

/// Acquire and release one pooled connection through the retrying acquire path.
///
/// # Errors
///
/// The acquire error: `FraiseQLError::ConnectionPool` on timeout or exhausted retries.
pub async fn acquire_connection_with_retry(adapter: &PostgresAdapter) -> Result<()> {
    adapter.acquire_connection_with_retry().await.map(drop)
}

/// Run `sql` (no parameters) through the adapter's raw read path with `routing`.
///
/// # Errors
///
/// As the raw read path: a query failure, or a `data` column that is not JSONB.
pub async fn execute_raw(
    adapter: &PostgresAdapter,
    sql: &str,
    routing: ReadRouting,
) -> Result<Vec<JsonbValue>> {
    adapter.execute_raw(sql, &[], routing).await
}

/// Record a write now, as every mutation-pipeline method does.
pub fn mark_write(adapter: &PostgresAdapter) {
    adapter.mark_write();
}

/// Decode column `column` of `row` with the adapter's binary `NUMERIC` decoder.
///
/// # Errors
///
/// The driver's decode error.
pub fn decode_numeric(
    row: &tokio_postgres::Row,
    column: &str,
) -> std::result::Result<String, tokio_postgres::Error> {
    row.try_get::<_, PgNumericText>(column).map(|n| n.0)
}
