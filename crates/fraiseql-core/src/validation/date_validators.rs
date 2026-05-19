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
pub(crate) fn parse_date(date_str: &str) -> Result<(u32, u32, u32)> {
    let parts: Vec<&str> = date_str.split('-').collect();
    if parts.len() != 3 {
        return Err(FraiseQLError::Validation {
            message: format!("Invalid date format: '{}'. Expected YYYY-MM-DD", date_str),
            path: None,
        });
    }

    let year = parts[0].parse::<u32>().map_err(|_| FraiseQLError::Validation {
        message: format!("Invalid year: '{}'", parts[0]),
        path: None,
    })?;

    let month = parts[1].parse::<u32>().map_err(|_| FraiseQLError::Validation {
        message: format!("Invalid month: '{}'", parts[1]),
        path: None,
    })?;

    let day = parts[2].parse::<u32>().map_err(|_| FraiseQLError::Validation {
        message: format!("Invalid day: '{}'", parts[2]),
        path: None,
    })?;

    if !(1..=12).contains(&month) {
        return Err(FraiseQLError::Validation {
            message: format!("Month must be between 1 and 12, got {}", month),
            path: None,
        });
    }

    let days_in_month = get_days_in_month(month, year);
    if !(1..=days_in_month).contains(&day) {
        return Err(FraiseQLError::Validation {
            message: format!("Day must be between 1 and {}, got {}", days_in_month, day),
            path: None,
        });
    }

    Ok((year, month, day))
}

/// Check if a year is a leap year.
pub(crate) const fn is_leap_year(year: u32) -> bool {
    (year.is_multiple_of(4) && !year.is_multiple_of(100)) || year.is_multiple_of(400)
}

/// Get the number of days in a month.
pub(crate) const fn get_days_in_month(month: u32, year: u32) -> u32 {
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
pub(crate) fn compare_dates(left: (u32, u32, u32), right: (u32, u32, u32)) -> i32 {
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
pub(crate) fn days_between(left: (u32, u32, u32), right: (u32, u32, u32)) -> i64 {
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
            path: None,
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
            path: None,
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
            path: None,
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
            path: None,
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
            path: None,
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
            path: None,
        });
    }

    Ok(())
}
