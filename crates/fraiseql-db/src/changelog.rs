//! Change-log contract: the canonical `duration_ms` computation, the
//! `fraiseql.started_at` session-var convention, and the data-quality marker.
//!
//! These are the single source of truth shared by the session-var resolver
//! (`fraiseql-core`), the adapter's `set_config` application (`fraiseql-db`), and
//! the executor's in-txn outbox write (Change Spine, phase-02). See
//! `docs/architecture/change-log-contract.md`.

#[cfg(test)]
mod tests;

/// The transaction-local PostgreSQL session variable holding the mutation start
/// timestamp, on the **DB clock** (`clock_timestamp()`).
pub const STARTED_AT_VAR: &str = "fraiseql.started_at";

/// Sentinel session-var value meaning "stamp this variable with the database's
/// `clock_timestamp()` at apply time," rather than binding the string literally.
///
/// The session-var resolver emits this for [`STARTED_AT_VAR`] so the start
/// timestamp is taken on the **same clock** (`clock_timestamp()`) used to close
/// the interval at the outbox write — eliminating app↔DB clock skew. The value
/// uses control characters so it can never collide with a real session value.
pub const CLOCK_TIMESTAMP_DIRECTIVE: &str = "\u{1}fraiseql:clock_timestamp\u{1}";

/// Data-quality marker for the `duration_ms` computation.
///
/// Stamped into a framework-written change-log row's
/// `extra_metadata->>'duration_calc_version'` and bumped when the computation
/// changes, so consumers (#392) can refuse to mix incomparable rows. `2` = the
/// wall-clock-correct, single-DB-clock computation ([`duration_ms_sql`]); legacy
/// app-written rows carry no marker (or `1`).
pub const DURATION_CALC_VERSION: i64 = 2;

/// The canonical SQL expression computing `duration_ms` as **full wall-clock
/// milliseconds** from `started_at` to now, on the DB clock.
///
/// Uses `EXTRACT(EPOCH FROM interval)` (total seconds) — **never**
/// `EXTRACT(MILLISECONDS FROM interval)`, which returns only the
/// seconds-within-the-minute × 1000 and so under-reports any interval ≥ 1
/// minute (`00:01:30.250` → `30250`, not `90250`).
///
/// `started_at_var` is a trusted GUC name (e.g. [`STARTED_AT_VAR`]); the result
/// reads it back with `current_setting(...)::timestamptz` and closes the
/// interval against `clock_timestamp()` — the same clock that set it.
///
/// # Example
///
/// ```
/// let sql = fraiseql_db::changelog::duration_ms_sql(fraiseql_db::changelog::STARTED_AT_VAR);
/// assert!(sql.contains("EXTRACT(EPOCH"));
/// assert!(!sql.contains("MILLISECONDS"));
/// ```
#[must_use]
pub fn duration_ms_sql(started_at_var: &str) -> String {
    format!(
        "(EXTRACT(EPOCH FROM (clock_timestamp() - current_setting('{started_at_var}')::timestamptz)) * 1000)::INTEGER"
    )
}
