//! Unit tests for the change-log duration computation (no database required).

use super::{
    CHANGELOG_PORTABLE_INSERT_COLUMNS, CLOCK_TIMESTAMP_DIRECTIVE, DURATION_CALC_VERSION,
    STARTED_AT_VAR, build_changelog_insert_sql, duration_ms_sql,
};
use crate::types::DatabaseType;

#[test]
fn duration_expr_uses_epoch_not_the_milliseconds_trap() {
    let sql = duration_ms_sql(STARTED_AT_VAR);
    assert!(
        sql.contains("EXTRACT(EPOCH"),
        "must use EXTRACT(EPOCH FROM interval) for full wall-clock seconds"
    );
    assert!(
        !sql.to_uppercase().contains("MILLISECONDS"),
        "must NOT use EXTRACT(MILLISECONDS) — it truncates intervals >= 1 minute"
    );
    assert!(sql.contains("* 1000"), "seconds are scaled to milliseconds");
    assert!(sql.contains("::INTEGER"), "result is an INTEGER duration_ms");
}

#[test]
fn duration_expr_reads_back_the_started_at_var_on_the_db_clock() {
    let sql = duration_ms_sql(STARTED_AT_VAR);
    assert!(
        sql.contains("current_setting('fraiseql.started_at')"),
        "reads the start timestamp back from the session var"
    );
    assert!(
        sql.contains("clock_timestamp()"),
        "closes the interval against the DB clock (same clock that set started_at)"
    );
}

#[test]
fn clock_directive_is_not_a_plausible_real_value() {
    // The sentinel must never collide with a real timestamp/string a caller
    // could legitimately set, hence the control-character framing.
    assert!(CLOCK_TIMESTAMP_DIRECTIVE.starts_with('\u{1}'));
    assert!(!CLOCK_TIMESTAMP_DIRECTIVE.contains(' '));
}

#[test]
fn duration_calc_version_is_the_post_fix_marker() {
    assert_eq!(DURATION_CALC_VERSION, 2);
}

// ── Portable outbox INSERT builder (multi-DB parity smoke construct) ─────────

#[test]
fn portable_insert_uses_dialect_specific_placeholders() {
    let table = "core.tb_entity_change_log";
    let n = CHANGELOG_PORTABLE_INSERT_COLUMNS.len();

    // PostgreSQL: $1..$N positional.
    let pg = build_changelog_insert_sql(table, DatabaseType::PostgreSQL);
    assert!(
        pg.contains("VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)"),
        "PG placeholders: {pg}"
    );

    // SQL Server: @P1..@PN.
    let mssql = build_changelog_insert_sql(table, DatabaseType::SQLServer);
    assert!(
        mssql.contains("VALUES (@P1, @P2, @P3, @P4, @P5, @P6, @P7, @P8, @P9, @P10, @P11)"),
        "MSSQL placeholders: {mssql}"
    );

    // MySQL and SQLite: anonymous `?`.
    for dialect in [DatabaseType::MySQL, DatabaseType::SQLite] {
        let sql = build_changelog_insert_sql(table, dialect);
        assert_eq!(sql.matches('?').count(), n, "{dialect} uses one `?` per column: {sql}");
        assert!(sql.contains("VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"), "{dialect}: {sql}");
    }
}

#[test]
fn portable_insert_writes_the_identity_and_envelope_subset() {
    let sql = build_changelog_insert_sql("t", DatabaseType::MySQL);
    // Target table + the contract column list (backtick-quoted for MySQL), in
    // lockstep with the constant.
    assert!(sql.starts_with("INSERT INTO t ("), "names the target table: {sql}");
    let cols = CHANGELOG_PORTABLE_INSERT_COLUMNS
        .iter()
        .map(|c| format!("`{c}`"))
        .collect::<Vec<_>>()
        .join(", ");
    assert!(sql.contains(&cols), "writes the portable contract columns `{cols}`: {sql}");
    // started_at/duration_ms are PG-request-scoped → omitted on the portable path.
    assert!(!sql.contains("started_at"), "started_at is omitted (PG-only): {sql}");
    assert!(!sql.contains("duration_ms"), "duration_ms is omitted (PG-only): {sql}");
    // seq comes from the table default, never the INSERT.
    assert!(!CHANGELOG_PORTABLE_INSERT_COLUMNS.contains(&"seq"), "seq is not in the INSERT");
    // trace_id is a plain text column → portable across every dialect (#375).
    assert!(
        CHANGELOG_PORTABLE_INSERT_COLUMNS.contains(&"trace_id"),
        "trace_id is written: {sql}"
    );
    // schema_version is a plain text column → portable across every dialect (#377).
    assert!(
        CHANGELOG_PORTABLE_INSERT_COLUMNS.contains(&"schema_version"),
        "schema_version is written: {sql}"
    );
    // trace_context is written as JSON text (JSON/JSONB/NVARCHAR(MAX) per dialect) (#375).
    assert!(
        CHANGELOG_PORTABLE_INSERT_COLUMNS.contains(&"trace_context"),
        "trace_context is written: {sql}"
    );
}

#[test]
fn portable_insert_quotes_identifiers_per_dialect() {
    // `cascade` is a reserved keyword in MySQL and SQL Server → must be quoted.
    let mysql = build_changelog_insert_sql("t", DatabaseType::MySQL);
    assert!(mysql.contains("`cascade`"), "MySQL backtick-quotes columns: {mysql}");
    let mssql = build_changelog_insert_sql("t", DatabaseType::SQLServer);
    assert!(mssql.contains("[cascade]"), "SQL Server bracket-quotes columns: {mssql}");
    let pg = build_changelog_insert_sql("t", DatabaseType::PostgreSQL);
    assert!(pg.contains("\"cascade\""), "PostgreSQL double-quotes columns: {pg}");
}
