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
/// Column identifiers are quoted per dialect (PostgreSQL/SQLite `"col"`, MySQL
/// `` `col` ``, SQL Server `[col]`) because `cascade` is a reserved keyword in
/// MySQL and SQL Server — an unquoted `cascade` is a syntax error there.
///
/// # Example
///
/// ```
/// use fraiseql_db::{changelog::build_changelog_insert_sql, DatabaseType};
/// let sql = build_changelog_insert_sql("core.tb_entity_change_log", DatabaseType::MySQL);
/// assert!(sql.starts_with("INSERT INTO core.tb_entity_change_log ("));
/// assert!(sql.contains("`cascade`"), "reserved word quoted for MySQL");
/// assert!(sql.contains("VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"));
/// ```
#[must_use]
pub fn build_changelog_insert_sql(table: &str, dialect: crate::types::DatabaseType) -> String {
    use crate::types::DatabaseType;
    let columns = CHANGELOG_PORTABLE_INSERT_COLUMNS;
    let quote_col = |c: &str| match dialect {
        DatabaseType::PostgreSQL | DatabaseType::SQLite => format!("\"{c}\""),
        DatabaseType::MySQL => format!("`{c}`"),
        DatabaseType::SQLServer => format!("[{c}]"),
    };
    let quoted_columns: Vec<String> = columns.iter().map(|c| quote_col(c)).collect();
    let placeholders: Vec<String> = (1..=columns.len())
        .map(|i| match dialect {
            DatabaseType::PostgreSQL => format!("${i}"),
            DatabaseType::SQLServer => format!("@P{i}"),
            DatabaseType::MySQL | DatabaseType::SQLite => "?".to_string(),
        })
        .collect();
    format!(
        "INSERT INTO {table} ({}) VALUES ({})",
        quoted_columns.join(", "),
        placeholders.join(", ")
    )
}

/// Permissive truthiness for a `mutation_response` boolean column, shared by the
/// portable (MySQL / SQL Server) outbox paths. Dialects surface `succeeded` /
/// `state_changed` differently — MySQL's `TRUE`/`FALSE` literals come back as
/// integers (binary protocol), SQL Server's `BIT` as a bool — so `true`, a
/// non-zero number, and the string forms all read as the same flag.
#[cfg(any(feature = "mysql", feature = "sqlserver"))]
#[must_use]
pub(crate) fn value_is_truthy(v: Option<&serde_json::Value>) -> bool {
    match v {
        Some(serde_json::Value::Bool(b)) => *b,
        Some(serde_json::Value::Number(n)) => {
            n.as_i64().is_some_and(|i| i != 0) || n.as_f64().is_some_and(|f| f != 0.0)
        },
        Some(serde_json::Value::String(s)) => s == "1" || s.eq_ignore_ascii_case("true"),
        _ => false,
    }
}

/// Serialise a JSON-bearing `mutation_response` column (`object_data`,
/// `updated_fields`, `cascade`) to text for binding to the portable outbox
/// INSERT's JSON / text column. A JSON `null` (or an absent column) binds as SQL
/// NULL; a non-string JSON value is re-serialised; a plain string passes through.
#[cfg(any(feature = "mysql", feature = "sqlserver"))]
#[must_use]
pub(crate) fn json_column_text(v: Option<&serde_json::Value>) -> Option<String> {
    match v {
        None | Some(serde_json::Value::Null) => None,
        Some(serde_json::Value::String(s)) => Some(s.clone()),
        Some(other) => Some(other.to_string()),
    }
}
