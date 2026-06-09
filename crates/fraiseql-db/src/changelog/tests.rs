//! Unit tests for the change-log duration computation (no database required).

use super::{CLOCK_TIMESTAMP_DIRECTIVE, DURATION_CALC_VERSION, STARTED_AT_VAR, duration_ms_sql};

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
