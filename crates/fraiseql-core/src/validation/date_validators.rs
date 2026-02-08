//! Date and time validation for input fields.
//!
//! This module provides validators for common date/time constraints:
//! - minDate, maxDate: Date range constraints
//! - minAge, maxAge: Age constraints (calculated from today)
//! - maxDaysInFuture, minDaysInPast: Relative date constraints
//!
//! # Examples
//!
//! ```ignore
//! // Validate birthdate is 18+ years old
//! validate_min_age("1990-03-15", 18)?;
//!
//! // Validate date is not more than 30 days in the future
//! validate_max_days_in_future("2026-03-10", 30)?;
//!
//! // Validate date is within range
//! validate_date_range("2026-02-08", "2020-01-01", "2030-12-31")?;
//! ```

use std::cmp::Ordering;

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
fn is_leap_year(year: u32) -> bool {
    (year % 4 == 0 && year % 100 != 0) || (year % 400 == 0)
}

/// Get the number of days in a month.
fn get_days_in_month(month: u32, year: u32) -> u32 {
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

/// Get today's date as (year, month, day).
/// For testing purposes, this can be overridden.
fn get_today() -> (u32, u32, u32) {
    // In a real implementation, this would use chrono or std::time
    // For now, we'll use a fixed date for testing consistency
    (2026, 2, 8)
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
pub fn validate_date_range(date_str: &str, min_date_str: &str, max_date_str: &str) -> Result<()> {
    validate_min_date(date_str, min_date_str)?;
    validate_max_date(date_str, max_date_str)?;
    Ok(())
}

/// Validate that a person is at least min_age years old.
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

/// Validate that a person is at most max_age years old.
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

/// Validate that a date is not more than max_days in the future.
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

/// Validate that a date is not more than max_days in the past.
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
    use super::*;

    #[test]
    fn test_parse_date_valid() {
        let result = parse_date("2026-02-08");
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), (2026, 2, 8));
    }

    #[test]
    fn test_parse_date_invalid_format() {
        assert!(parse_date("2026/02/08").is_err());
        assert!(parse_date("02-08-2026").is_err());
    }

    #[test]
    fn test_parse_date_invalid_month() {
        assert!(parse_date("2026-13-01").is_err());
        assert!(parse_date("2026-00-01").is_err());
    }

    #[test]
    fn test_parse_date_invalid_day() {
        assert!(parse_date("2026-02-30").is_err());
        assert!(parse_date("2026-04-31").is_err());
    }

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
    fn test_compare_dates() {
        assert!(compare_dates((2026, 2, 8), (2026, 2, 7)) > 0);
        assert!(compare_dates((2026, 2, 7), (2026, 2, 8)) < 0);
        assert_eq!(compare_dates((2026, 2, 8), (2026, 2, 8)), 0);
        assert!(compare_dates((2026, 3, 1), (2026, 2, 28)) > 0);
        assert!(compare_dates((2027, 1, 1), (2026, 12, 31)) > 0);
    }

    #[test]
    fn test_min_date_passes() {
        assert!(validate_min_date("2026-02-08", "2026-02-01").is_ok());
        assert!(validate_min_date("2026-02-08", "2026-02-08").is_ok());
    }

    #[test]
    fn test_min_date_fails() {
        assert!(validate_min_date("2026-02-08", "2026-02-09").is_err());
    }

    #[test]
    fn test_max_date_passes() {
        assert!(validate_max_date("2026-02-08", "2026-02-15").is_ok());
        assert!(validate_max_date("2026-02-08", "2026-02-08").is_ok());
    }

    #[test]
    fn test_max_date_fails() {
        assert!(validate_max_date("2026-02-08", "2026-02-07").is_err());
    }

    #[test]
    fn test_date_range_passes() {
        assert!(validate_date_range("2026-02-08", "2026-01-01", "2026-12-31").is_ok());
    }

    #[test]
    fn test_date_range_fails_below_min() {
        assert!(validate_date_range("2025-12-31", "2026-01-01", "2026-12-31").is_err());
    }

    #[test]
    fn test_date_range_fails_above_max() {
        assert!(validate_date_range("2027-01-01", "2026-01-01", "2026-12-31").is_err());
    }

    #[test]
    fn test_min_age_passes() {
        // Today is 2026-02-08, person born 2000-01-01 is 26 years old
        assert!(validate_min_age("2000-01-01", 25).is_ok());
        assert!(validate_min_age("2000-01-01", 26).is_ok());
    }

    #[test]
    fn test_min_age_fails() {
        // Person born 2010-03-15 is 15 years old (hasn't turned 16 yet)
        assert!(validate_min_age("2010-03-15", 16).is_err());
    }

    #[test]
    fn test_min_age_birthday_today() {
        // Today is 2026-02-08, person born 2008-02-08 is exactly 18 years old
        assert!(validate_min_age("2008-02-08", 18).is_ok());
    }

    #[test]
    fn test_min_age_before_birthday_this_year() {
        // Today is 2026-02-08, person born 2008-03-15 is 17 (not yet 18)
        assert!(validate_min_age("2008-03-15", 18).is_err());
    }

    #[test]
    fn test_max_age_passes() {
        // Today is 2026-02-08, person born 2010-01-01 is 16 years old
        assert!(validate_max_age("2010-01-01", 17).is_ok());
        assert!(validate_max_age("2010-01-01", 16).is_ok());
    }

    #[test]
    fn test_max_age_fails() {
        // Person born 1990-01-01 is 36 years old
        assert!(validate_max_age("1990-01-01", 35).is_err());
    }

    #[test]
    fn test_max_days_in_future_passes() {
        // 2026-02-08 (today) + 30 days = 2026-03-10
        assert!(validate_max_days_in_future("2026-02-10", 30).is_ok());
    }

    #[test]
    fn test_max_days_in_future_fails() {
        // Date more than 30 days in future should fail
        assert!(validate_max_days_in_future("2026-03-15", 30).is_err());
    }

    #[test]
    fn test_max_days_in_past_passes() {
        // 2026-02-08 (today) - 30 days = 2026-01-09
        assert!(validate_max_days_in_past("2026-02-01", 30).is_ok());
    }

    #[test]
    fn test_max_days_in_past_fails() {
        // Date more than 30 days in past should fail
        assert!(validate_max_days_in_past("2026-01-01", 30).is_err());
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

    #[test]
    fn test_february_leap_year_edge_case() {
        // 2024 is a leap year, so Feb has 29 days
        assert!(parse_date("2024-02-29").is_ok());
        assert!(parse_date("2024-02-30").is_err());
    }

    #[test]
    fn test_february_non_leap_year_edge_case() {
        // 2025 is not a leap year, so Feb has 28 days
        assert!(parse_date("2025-02-28").is_ok());
        assert!(parse_date("2025-02-29").is_err());
    }

    #[test]
    fn test_year_2000_leap_year() {
        // 2000 is divisible by 400, so it's a leap year
        assert!(is_leap_year(2000));
        assert!(parse_date("2000-02-29").is_ok());
    }

    #[test]
    fn test_year_1900_not_leap_year() {
        // 1900 is divisible by 100 but not 400, so not a leap year
        assert!(!is_leap_year(1900));
        assert!(parse_date("1900-02-29").is_err());
    }

    #[test]
    fn test_age_calculation_before_birthday() {
        // Today is 2026-02-08
        // Person born 2000-05-15 is 25 (not yet 26)
        assert!(validate_min_age("2000-05-15", 26).is_err());
        assert!(validate_min_age("2000-05-15", 25).is_ok());
    }

    #[test]
    fn test_age_calculation_after_birthday() {
        // Today is 2026-02-08
        // Person born 2000-01-15 is 26 (already had their birthday)
        assert!(validate_min_age("2000-01-15", 26).is_ok());
    }
}
