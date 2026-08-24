#![allow(clippy::unwrap_used, clippy::panic)] // Reason: test code, panics are acceptable

//! Unit tests for the Arrow-value → SQL-literal conversion.
//!
//! These live beside `convert.rs` so they can reach the private
//! `arrow_value_to_sql` directly. The values here are entirely client-controlled:
//! they arrive as IPC bytes on `do_put` / `do_exchange`, so every arm must reject
//! an out-of-range value rather than panic (#1040).

use std::sync::Arc;

use arrow::array::{Array, Date32Array};

use super::arrow_value_to_sql;

fn date32_column(value: i32) -> Arc<dyn Array> {
    Arc::new(Date32Array::from(vec![value]))
}

/// An ordinary date still converts — the guard must not reject valid input.
#[test]
fn date32_epoch_converts() {
    let sql = arrow_value_to_sql(&date32_column(0), 0).unwrap();
    assert_eq!(sql, "'1970-01-01'::date");
}

/// A realistic modern date still converts.
#[test]
fn date32_modern_date_converts() {
    // 20 000 days after the epoch is 2024-10-04.
    let sql = arrow_value_to_sql(&date32_column(20_000), 0).unwrap();
    assert_eq!(sql, "'2024-10-04'::date");
}

/// #1040 — `i32::MAX` days is a legal Arrow `Date32` and roughly year 5 881 580,
/// far past `NaiveDate::MAX`. `NaiveDate + TimeDelta` is `checked_add_signed(..)
/// .expect(..)`, so this panicked inside the spawned `do_put` task, ending the
/// RPC with grpc-status OK and nothing written.
#[test]
fn date32_max_is_rejected_not_panicking() {
    let result = arrow_value_to_sql(&date32_column(i32::MAX), 0);
    assert!(result.is_err(), "an out-of-range Date32 must be an error, not a panic");
}

/// The negative side overflows identically.
#[test]
fn date32_min_is_rejected_not_panicking() {
    let result = arrow_value_to_sql(&date32_column(i32::MIN), 0);
    assert!(result.is_err(), "an out-of-range negative Date32 must be an error, not a panic");
}

/// The trigger is far wider than `i32::MAX`, and does not need a hostile client.
///
/// `NaiveDate::MAX` is about 95 million days from the epoch, so any value beyond
/// that overflows — including the ordinary unit mix-up of writing a Unix-epoch
/// *seconds* value into a `Date32` column. 1 700 000 000 fits in an `i32` and is
/// ~18x over the limit, so an honest buggy client hits this as easily as an
/// attacker.
#[test]
fn date32_unix_seconds_mistake_is_rejected_not_panicking() {
    let result = arrow_value_to_sql(&date32_column(1_700_000_000), 0);
    assert!(
        result.is_err(),
        "a seconds-valued Date32 must be an error, not a panic — this is a plausible client bug"
    );
}

/// The rejection must say what was wrong, so an operator can tell this apart from
/// a generic conversion failure.
#[test]
fn date32_overflow_error_names_the_offending_value() {
    let err = arrow_value_to_sql(&date32_column(i32::MAX), 0).unwrap_err();
    assert!(
        err.contains(&i32::MAX.to_string()),
        "the error should name the offending value, got: {err}"
    );
}
