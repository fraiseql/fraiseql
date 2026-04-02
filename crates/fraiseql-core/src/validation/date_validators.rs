//! Date and time validation for input fields.
//!
//! This module provides validators for common date/time constraints:
//! - minDate, maxDate: Date range constraints
//! - minAge, maxAge: Age constraints (calculated from today)
//! - maxDaysInFuture, minDaysInPast: Relative date constraints
//!
//! # Examples
//!
//! ```
//! use fraiseql_core::validation::{validate_min_age, validate_max_days_in_future, validate_date_range};
//!
//! // Validate birthdate is 18+ years old
//! validate_min_age("1990-03-15", 18).unwrap();
//!
//! // Validate date is not more than 30 days in the future
//! validate_max_days_in_future("2026-03-10", 30).unwrap();
//!
//! // Validate date is within range
//! validate_date_range("2026-02-08", "2020-01-01", "2030-12-31").unwrap();
//! ```

use std::cmp::Ordering;

use chrono::Datelike;

use crate::error::{FraiseQLError, Result};

/// Parse a date string in ISO 8601 format (YYYY-MM-DD).
fn parse_date(date_str: &str) -> Result<(u32, u32, u32)> {
    let parts: Vec<&str> = date_str.split('-').collect();
    if parts.len() != 3 {
        return Err(FraiseQLError::Validation {
            message: format!("Invalid date format: '{}'. Expected YYYY-MM-DD", date_str),
            path:    None,
        });
    }

    let year = parts[0].parse::<u32>().map_err(|_| FraiseQLError::Validation {
        message: format!("Invalid year: '{}'", parts[0]),
        path:    None,
    })?;

    let month = parts[1].parse::<u32>().map_err(|_| FraiseQLError::Validation {
        message: format!("Invalid month: '{}'", parts[1]),
        path:    None,
    })?;

    let day = parts[2].parse::<u32>().map_err(|_| FraiseQLError::Validation {
        message: format!("Invalid day: '{}'", parts[2]),
        path:    None,
    })?;

    if !(1..=12).contains(&month) {
        return Err(FraiseQLError::Validation {
            message: format!("Month must be between 1 and 12, got {}", month),
            path:    None,
        });
    }

    let days_in_month = get_days_in_month(month, year);
    if !(1..=days_in_month).contains(&day) {
        return Err(FraiseQLError::Validation {
            message: format!("Day must be between 1 and {}, got {}", days_in_month, day),
            path:    None,
        });
    }

    Ok((year, month, day))
}

/// Check if a year is a leap year.
const fn is_leap_year(year: u32) -> bool {
    (year.is_multiple_of(4) && !year.is_multiple_of(100)) || year.is_multiple_of(400)
}

/// Get the number of days in a month.
const fn get_days_in_month(month: u32, year: u32) -> u32 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 => {
            if is_leap_year(year) {
                29
            } else {
                28
            }
        },
        _ => 0,
    }
}

/// Get today's date as (year, month, day) in UTC.
fn get_today() -> (u32, u32, u32) {
    let today = chrono::Utc::now().date_naive();
    (today.year_ce().1, today.month(), today.day())
}

/// Compare two dates: -1 if left < right, 0 if equal, 1 if left > right.
fn compare_dates(left: (u32, u32, u32), right: (u32, u32, u32)) -> i32 {
    match left.0.cmp(&right.0) {
        Ordering::Less => -1,
        Ordering::Greater => 1,
        Ordering::Equal => match left.1.cmp(&right.1) {
            Ordering::Less => -1,
            Ordering::Greater => 1,
            Ordering::Equal => match left.2.cmp(&right.2) {
                Ordering::Less => -1,
                Ordering::Greater => 1,
                Ordering::Equal => 0,
            },
        },
    }
}

/// Calculate the number of days between two dates (left - right).
fn days_between(left: (u32, u32, u32), right: (u32, u32, u32)) -> i64 {
    // Simple day count from year 0 to avoid floating point
    let days_left = i64::from(left.0) * 365 + i64::from(left.1) * 31 + i64::from(left.2);
    let days_right = i64::from(right.0) * 365 + i64::from(right.1) * 31 + i64::from(right.2);
    days_left - days_right
}

/// Validate that a date is >= minimum date.
///
/// # Errors
///
/// Returns [`FraiseQLError::Validation`] if either date string is not valid
/// ISO 8601 (YYYY-MM-DD) or if `date_str` is earlier than `min_date_str`.
pub fn validate_min_date(date_str: &str, min_date_str: &str) -> Result<()> {
    let date = parse_date(date_str)?;
    let min_date = parse_date(min_date_str)?;

    if compare_dates(date, min_date) < 0 {
        return Err(FraiseQLError::Validation {
            message: format!("Date '{}' must be >= '{}'", date_str, min_date_str),
            path:    None,
        });
    }

    Ok(())
}

/// Validate that a date is <= maximum date.
///
/// # Errors
///
/// Returns [`FraiseQLError::Validation`] if either date string is not valid
/// ISO 8601 (YYYY-MM-DD) or if `date_str` is later than `max_date_str`.
pub fn validate_max_date(date_str: &str, max_date_str: &str) -> Result<()> {
    let date = parse_date(date_str)?;
    let max_date = parse_date(max_date_str)?;

    if compare_dates(date, max_date) > 0 {
        return Err(FraiseQLError::Validation {
            message: format!("Date '{}' must be <= '{}'", date_str, max_date_str),
            path:    None,
        });
    }

    Ok(())
}

/// Validate that a date is within a range (inclusive).
///
/// # Errors
///
/// Returns [`FraiseQLError::Validation`] if any date string is invalid, if
/// `date_str` is before `min_date_str`, or if `date_str` is after `max_date_str`.
pub fn validate_date_range(date_str: &str, min_date_str: &str, max_date_str: &str) -> Result<()> {
    validate_min_date(date_str, min_date_str)?;
    validate_max_date(date_str, max_date_str)?;
    Ok(())
}

/// Validate that a person is at least `min_age` years old.
///
/// # Errors
///
/// Returns [`FraiseQLError::Validation`] if `date_str` is not valid ISO 8601
/// (YYYY-MM-DD) or if the calculated age is less than `min_age`.
pub fn validate_min_age(date_str: &str, min_age: u32) -> Result<()> {
    let birth_date = parse_date(date_str)?;
    let today = get_today();

    // Calculate age
    let mut age = today.0 - birth_date.0;
    if (today.1, today.2) < (birth_date.1, birth_date.2) {
        age -= 1;
    }

    if age < min_age {
        return Err(FraiseQLError::Validation {
            message: format!("Age must be at least {} years old, got {}", min_age, age),
            path:    None,
        });
    }

    Ok(())
}

/// Validate that a person is at most `max_age` years old.
///
/// # Errors
///
/// Returns [`FraiseQLError::Validation`] if `date_str` is not valid ISO 8601
/// (YYYY-MM-DD) or if the calculated age exceeds `max_age`.
pub fn validate_max_age(date_str: &str, max_age: u32) -> Result<()> {
    let birth_date = parse_date(date_str)?;
    let today = get_today();

    // Calculate age
    let mut age = today.0 - birth_date.0;
    if (today.1, today.2) < (birth_date.1, birth_date.2) {
        age -= 1;
    }

    if age > max_age {
        return Err(FraiseQLError::Validation {
            message: format!("Age must be at most {} years old, got {}", max_age, age),
            path:    None,
        });
    }

    Ok(())
}

/// Validate that a date is not more than `max_days` in the future.
///
/// # Errors
///
/// Returns [`FraiseQLError::Validation`] if `date_str` is not valid ISO 8601
/// (YYYY-MM-DD) or if the date is more than `max_days` days in the future.
pub fn validate_max_days_in_future(date_str: &str, max_days: i64) -> Result<()> {
    let date = parse_date(date_str)?;
    let today = get_today();

    let days_diff = days_between(date, today);
    if days_diff > max_days {
        return Err(FraiseQLError::Validation {
            message: format!(
                "Date '{}' cannot be more than {} days in the future",
                date_str, max_days
            ),
            path:    None,
        });
    }

    Ok(())
}

/// Validate that a date is not more than `max_days` in the past.
///
/// # Errors
///
/// Returns [`FraiseQLError::Validation`] if `date_str` is not valid ISO 8601
/// (YYYY-MM-DD) or if the date is more than `max_days` days in the past.
pub fn validate_max_days_in_past(date_str: &str, max_days: i64) -> Result<()> {
    let date = parse_date(date_str)?;
    let today = get_today();

    let days_diff = days_between(today, date);
    if days_diff > max_days {
        return Err(FraiseQLError::Validation {
            message: format!(
                "Date '{}' cannot be more than {} days in the past",
                date_str, max_days
            ),
            path:    None,
        });
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

    use chrono::Datelike;

    use super::*;

    // ── Helpers for time-independent tests ──────────────────────────────────

    /// Returns "YYYY-MM-DD" for `years` years before today.
    fn years_ago(years: u32) -> String {
        let today = chrono::Utc::now().date_naive();
        let y = today.year() - i32::try_from(years).unwrap_or(0);
        format!("{y}-{:02}-{:02}", today.month(), today.day())
    }

    /// Returns "YYYY-MM-DD" for today.
    fn today_str() -> String {
        chrono::Utc::now().date_naive().format("%Y-%m-%d").to_string()
    }

    // ── parse_date ───────────────────────────────────────────────────────────

    #[test]
    fn test_parse_date_valid() {
        let result = parse_date("2026-02-08");
        let parsed = result.unwrap_or_else(|e| panic!("valid date should parse: {e}"));
        assert_eq!(parsed, (2026, 2, 8));
    }

    #[test]
    fn test_parse_date_invalid_format() {
        assert!(
            matches!(parse_date("2026/02/08"), Err(FraiseQLError::Validation { .. })),
            "slash-separated date should fail parsing"
        );
        assert!(
            matches!(parse_date("02-08-2026"), Err(FraiseQLError::Validation { .. })),
            "MM-DD-YYYY format should fail parsing"
        );
    }

    #[test]
    fn test_parse_date_invalid_month() {
        assert!(
            matches!(parse_date("2026-13-01"), Err(FraiseQLError::Validation { .. })),
            "month 13 should fail validation"
        );
        assert!(
            matches!(parse_date("2026-00-01"), Err(FraiseQLError::Validation { .. })),
            "month 0 should fail validation"
        );
    }

    #[test]
    fn test_parse_date_invalid_day() {
        assert!(
            matches!(parse_date("2026-02-30"), Err(FraiseQLError::Validation { .. })),
            "Feb 30 should fail validation"
        );
        assert!(
            matches!(parse_date("2026-04-31"), Err(FraiseQLError::Validation { .. })),
            "Apr 31 should fail validation"
        );
    }

    // ── leap year / days in month ────────────────────────────────────────────

    #[test]
    fn test_leap_year_detection() {
        assert!(is_leap_year(2024));
        assert!(is_leap_year(2000));
        assert!(!is_leap_year(1900));
        assert!(!is_leap_year(2025));
    }

    #[test]
    fn test_days_in_month() {
        assert_eq!(get_days_in_month(1, 2026), 31);
        assert_eq!(get_days_in_month(2, 2024), 29); // Leap year
        assert_eq!(get_days_in_month(2, 2026), 28); // Non-leap year
        assert_eq!(get_days_in_month(4, 2026), 30);
    }

    #[test]
    fn test_february_leap_year_edge_case() {
        parse_date("2024-02-29")
            .unwrap_or_else(|e| panic!("Feb 29 on leap year should parse: {e}"));
        assert!(
            matches!(parse_date("2024-02-30"), Err(FraiseQLError::Validation { .. })),
            "Feb 30 on leap year should fail"
        );
    }

    #[test]
    fn test_february_non_leap_year_edge_case() {
        parse_date("2025-02-28")
            .unwrap_or_else(|e| panic!("Feb 28 on non-leap year should parse: {e}"));
        assert!(
            matches!(parse_date("2025-02-29"), Err(FraiseQLError::Validation { .. })),
            "Feb 29 on non-leap year should fail"
        );
    }

    #[test]
    fn test_year_2000_leap_year() {
        assert!(is_leap_year(2000));
        parse_date("2000-02-29").unwrap_or_else(|e| panic!("Feb 29 in 2000 should parse: {e}"));
    }

    #[test]
    fn test_year_1900_not_leap_year() {
        assert!(!is_leap_year(1900));
        assert!(
            matches!(parse_date("1900-02-29"), Err(FraiseQLError::Validation { .. })),
            "Feb 29 in 1900 (not leap) should fail"
        );
    }

    // ── compare_dates / days_between ────────────────────────────────────────

    #[test]
    fn test_compare_dates() {
        assert!(compare_dates((2026, 2, 8), (2026, 2, 7)) > 0);
        assert!(compare_dates((2026, 2, 7), (2026, 2, 8)) < 0);
        assert_eq!(compare_dates((2026, 2, 8), (2026, 2, 8)), 0);
        assert!(compare_dates((2026, 3, 1), (2026, 2, 28)) > 0);
        assert!(compare_dates((2027, 1, 1), (2026, 12, 31)) > 0);
    }

    #[test]
    fn test_days_between_same_date() {
        assert_eq!(days_between((2026, 2, 8), (2026, 2, 8)), 0);
    }

    #[test]
    fn test_days_between_year_difference() {
        let diff = days_between((2027, 2, 8), (2026, 2, 8));
        assert!(diff > 0);
    }

    // ── validate_min_date / validate_max_date / validate_date_range ─────────

    #[test]
    fn test_min_date_passes() {
        validate_min_date("2026-02-08", "2026-02-01")
            .unwrap_or_else(|e| panic!("date after min should pass: {e}"));
        validate_min_date("2026-02-08", "2026-02-08")
            .unwrap_or_else(|e| panic!("date equal to min should pass: {e}"));
    }

    #[test]
    fn test_min_date_fails() {
        let result = validate_min_date("2026-02-08", "2026-02-09");
        assert!(
            matches!(result, Err(FraiseQLError::Validation { .. })),
            "date before min should fail, got: {result:?}"
        );
    }

    #[test]
    fn test_max_date_passes() {
        validate_max_date("2026-02-08", "2026-02-15")
            .unwrap_or_else(|e| panic!("date before max should pass: {e}"));
        validate_max_date("2026-02-08", "2026-02-08")
            .unwrap_or_else(|e| panic!("date equal to max should pass: {e}"));
    }

    #[test]
    fn test_max_date_fails() {
        let result = validate_max_date("2026-02-08", "2026-02-07");
        assert!(
            matches!(result, Err(FraiseQLError::Validation { .. })),
            "date after max should fail, got: {result:?}"
        );
    }

    #[test]
    fn test_date_range_passes() {
        validate_date_range("2026-02-08", "2026-01-01", "2026-12-31")
            .unwrap_or_else(|e| panic!("date within range should pass: {e}"));
    }

    #[test]
    fn test_date_range_fails_below_min() {
        let result = validate_date_range("2025-12-31", "2026-01-01", "2026-12-31");
        assert!(
            matches!(result, Err(FraiseQLError::Validation { .. })),
            "date below range should fail, got: {result:?}"
        );
    }

    #[test]
    fn test_date_range_fails_above_max() {
        let result = validate_date_range("2027-01-01", "2026-01-01", "2026-12-31");
        assert!(
            matches!(result, Err(FraiseQLError::Validation { .. })),
            "date above range should fail, got: {result:?}"
        );
    }

    // ── validate_min_age / validate_max_age (time-independent) ──────────────

    #[test]
    fn test_min_age_passes_clearly_old_enough() {
        // Born 50 years ago: definitely passes min_age = 18
        validate_min_age(&years_ago(50), 18)
            .unwrap_or_else(|e| panic!("50yo should pass min_age=18: {e}"));
    }

    #[test]
    fn test_min_age_fails_too_young() {
        // Born 5 years ago: cannot pass min_age = 18
        let result = validate_min_age(&years_ago(5), 18);
        assert!(
            matches!(result, Err(FraiseQLError::Validation { .. })),
            "5yo should fail min_age=18, got: {result:?}"
        );
    }

    #[test]
    fn test_min_age_birthday_today_exactly_18() {
        // Born exactly 18 years ago today → passes min_age = 18
        validate_min_age(&years_ago(18), 18)
            .unwrap_or_else(|e| panic!("exactly 18yo should pass min_age=18: {e}"));
    }

    #[test]
    fn test_max_age_passes_clearly_young_enough() {
        // Born 5 years ago: definitely passes max_age = 18
        validate_max_age(&years_ago(5), 18)
            .unwrap_or_else(|e| panic!("5yo should pass max_age=18: {e}"));
    }

    #[test]
    fn test_max_age_fails_too_old() {
        // Born 100 years ago: cannot pass max_age = 90
        let result = validate_max_age(&years_ago(100), 90);
        assert!(
            matches!(result, Err(FraiseQLError::Validation { .. })),
            "100yo should fail max_age=90, got: {result:?}"
        );
    }

    // ── validate_max_days_in_future / validate_max_days_in_past ─────────────

    #[test]
    fn test_max_days_in_future_today_passes() {
        // Today is 0 days in the future — always passes
        validate_max_days_in_future(&today_str(), 0)
            .unwrap_or_else(|e| panic!("today should pass max_days_in_future=0: {e}"));
    }

    #[test]
    fn test_max_days_in_future_past_date_passes() {
        // A date in 2000 is never in the future
        validate_max_days_in_future("2000-01-01", 0)
            .unwrap_or_else(|e| panic!("past date should pass max_days_in_future: {e}"));
    }

    #[test]
    fn test_max_days_in_future_far_future_fails() {
        // Year 9999 is always more than 30 days in the future
        let result = validate_max_days_in_future("9999-12-31", 30);
        assert!(
            matches!(result, Err(FraiseQLError::Validation { .. })),
            "year 9999 should fail max_days_in_future=30, got: {result:?}"
        );
    }

    #[test]
    fn test_max_days_in_past_today_passes() {
        // Today is 0 days in the past — always passes
        validate_max_days_in_past(&today_str(), 0)
            .unwrap_or_else(|e| panic!("today should pass max_days_in_past=0: {e}"));
    }

    #[test]
    fn test_max_days_in_past_far_past_fails() {
        // A date 50 years ago is more than 30 days in the past
        let result = validate_max_days_in_past(&years_ago(50), 30);
        assert!(
            matches!(result, Err(FraiseQLError::Validation { .. })),
            "50 years ago should fail max_days_in_past=30, got: {result:?}"
        );
    }
}
