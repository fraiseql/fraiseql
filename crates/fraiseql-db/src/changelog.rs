//! Change-log contract: the canonical `duration_ms` computation, the
//! `fraiseql.started_at` session-var convention, and the data-quality marker.
//!
//! These are the single source of truth shared by the session-var resolver
//! (`fraiseql-core`), the adapter's `set_config` application (`fraiseql-db`), and
//! the executor's in-txn outbox write (the Change Spine). See
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

/// The transaction-local PostgreSQL session variable that marks a write as
/// **FraiseQL-mediated** (#366).
///
/// The mutation executor sets this to [`CDC_MEDIATED_ON`] at the start of every
/// mutation transaction (PostgreSQL only). The shipped fallback-capture trigger
/// `core.fn_entity_change_log_capture()` reads it with
/// `current_setting('fraiseql.cdc_mediated', true)` and **suppresses** its own
/// change-log row when it equals [`CDC_MEDIATED_ON`] — so an app-path mutation,
/// already logged by the in-transaction outbox, is never double-captured. A raw
/// external write (psql / a migration / a third-party tool) leaves the GUC unset,
/// so the trigger fires and captures the change. Dotted custom GUC: no
/// `postgresql.conf` declaration is required, exactly like [`STARTED_AT_VAR`].
pub const CDC_MEDIATED_VAR: &str = "fraiseql.cdc_mediated";

/// The value [`CDC_MEDIATED_VAR`] carries when a write is FraiseQL-mediated.
///
/// The fallback-capture trigger suppresses its row only on an exact match, so the
/// unset state (raw external writes → `current_setting(..., true)` is NULL) never
/// suppresses capture.
pub const CDC_MEDIATED_ON: &str = "on";

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

/// The columns a **portable** (non-PostgreSQL) outbox INSERT writes.
///
/// The changed-entity identity + the Change Spine envelope subset that any
/// dialect — and any cooperative external producer — can supply by value.
///
/// PostgreSQL writes a richer set via its in-txn `MATERIALIZED` CTE (it also
/// stamps `started_at`/`duration_ms` from the request-scoped GUC, computed in
/// SQL). Those two columns are PostgreSQL-request-scoped and are **legitimately
/// omitted (NULL)** on the portable path — exactly the rows #392's `null-rate`
/// subcommand expects from non-FraiseQL producers. `seq` is supplied by the
/// table's sequence/identity default, never by the INSERT.
pub const CHANGELOG_PORTABLE_INSERT_COLUMNS: &[&str] = &[
    "object_type",
    "modification_type",
    "object_id",
    "object_data",
    "updated_fields",
    "cascade",
    "tenant_id",
    "trace_id",
    "schema_version",
    "trace_context",
    "actor_type",
    "acting_for",
    "commit_time",
];

/// Build a portable, fully-parameterized outbox INSERT for a non-PostgreSQL dialect.
///
/// The multi-DB counterpart of PostgreSQL's in-txn CTE: the row values are bound
/// from the parsed `app.mutation_response` row in Rust, since MySQL / SQL Server
/// cannot reference a `CALL`/`EXEC` result set in a following `INSERT ... SELECT`.
///
/// Placeholders are dialect-specific: PostgreSQL `$1, $2, …`, SQL Server
/// `@P1, @P2, …`, MySQL / SQLite `?`. The column list is
/// [`CHANGELOG_PORTABLE_INSERT_COLUMNS`], so every dialect writes the same
/// contract shape.
///
/// Column identifiers are double-quoted because `cascade` is a reserved keyword —
/// an unquoted `cascade` is a syntax error.
///
/// # Example
///
/// ```
/// use fraiseql_db::{changelog::build_changelog_insert_sql, DatabaseType};
/// let sql = build_changelog_insert_sql("core.tb_entity_change_log", DatabaseType::PostgreSQL);
/// assert!(sql.starts_with("INSERT INTO core.tb_entity_change_log ("));
/// assert!(sql.contains("\"cascade\""), "reserved word quoted");
/// assert!(sql.contains("VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13)"));
/// ```
#[must_use]
pub fn build_changelog_insert_sql(table: &str, dialect: crate::types::DatabaseType) -> String {
    use crate::types::DatabaseType;
    let columns = CHANGELOG_PORTABLE_INSERT_COLUMNS;
    let quote_col = |c: &str| match dialect {
        DatabaseType::PostgreSQL => format!("\"{c}\""),
    };
    let quoted_columns: Vec<String> = columns.iter().map(|c| quote_col(c)).collect();
    let placeholders: Vec<String> = (1..=columns.len())
        .map(|i| match dialect {
            DatabaseType::PostgreSQL => format!("${i}"),
        })
        .collect();
    format!(
        "INSERT INTO {table} ({}) VALUES ({})",
        quoted_columns.join(", "),
        placeholders.join(", ")
    )
}
