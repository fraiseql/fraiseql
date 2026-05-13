//! Partial-period awareness for pre-aggregated time-series views.
//!
//! When a date filter is applied to a coarse-grain view (e.g. monthly aggregates),
//! the lower-bound date may fall in the middle of a period. This module provides
//! helpers to detect that situation and build UNION ALL queries that combine:
//!
//! - **Branch 1**: fine-grain rows for the partial leading period (when present)
//! - **Branch 2**: coarse-grain rows for complete intermediate periods
//! - **Branch 3**: fine-grain rows for the current in-progress period
//!
//! All period arithmetic functions are pure (no database calls, no side effects).

use chrono::{Datelike, NaiveDate, TimeDelta};

use crate::compiler::fact_table::TemporalGrain;

/// Determines whether a date falls exactly on a period boundary.
///
/// Period boundaries:
/// - **Day**: every date is day-aligned
/// - **Week**: Monday only (ISO week start)
/// - **Month**: first day of month
/// - **Quarter**: first day of a quarter month (Jan, Apr, Jul, Oct)
/// - **Year**: January 1st
///
/// # Examples
///
/// ```
/// use chrono::NaiveDate;
/// use fraiseql_core::compiler::fact_table::TemporalGrain;
/// use fraiseql_core::runtime::partial_period::is_period_aligned;
///
/// let jan1 = NaiveDate::from_ymd_opt(2024, 1, 1).unwrap();
/// assert!(is_period_aligned(jan1, TemporalGrain::Month));
/// assert!(is_period_aligned(jan1, TemporalGrain::Quarter));
/// assert!(is_period_aligned(jan1, TemporalGrain::Year));
///
/// let jan15 = NaiveDate::from_ymd_opt(2024, 1, 15).unwrap();
/// assert!(!is_period_aligned(jan15, TemporalGrain::Month));
/// ```
#[must_use]
pub fn is_period_aligned(date: NaiveDate, grain: TemporalGrain) -> bool {
    match grain {
        TemporalGrain::Day => true,
        TemporalGrain::Week => date.weekday() == chrono::Weekday::Mon,
        TemporalGrain::Month => date.day() == 1,
        TemporalGrain::Quarter => date.day() == 1 && (date.month() - 1).is_multiple_of(3),
        TemporalGrain::Year => date.ordinal() == 1,
    }
}

/// Returns the start of the period containing the given date.
///
/// # Examples
///
/// ```
/// use chrono::NaiveDate;
/// use fraiseql_core::compiler::fact_table::TemporalGrain;
/// use fraiseql_core::runtime::partial_period::period_start;
///
/// let d = NaiveDate::from_ymd_opt(2024, 2, 15).unwrap();
/// assert_eq!(
///     period_start(d, TemporalGrain::Month),
///     NaiveDate::from_ymd_opt(2024, 2, 1).unwrap()
/// );
/// ```
///
/// # Panics
///
/// Panics if the computed date is outside the `NaiveDate` range, which cannot
/// happen for practical calendar dates (years 0–9999).
#[must_use]
pub fn period_start(date: NaiveDate, grain: TemporalGrain) -> NaiveDate {
    match grain {
        TemporalGrain::Day => date,
        TemporalGrain::Week => {
            let days_since_monday = date.weekday().num_days_from_monday();
            date - TimeDelta::days(i64::from(days_since_monday))
        }
        TemporalGrain::Month => {
            NaiveDate::from_ymd_opt(date.year(), date.month(), 1)
                .expect("day 1 of any month is valid")
        }
        TemporalGrain::Quarter => {
            let quarter_month = ((date.month() - 1) / 3) * 3 + 1;
            NaiveDate::from_ymd_opt(date.year(), quarter_month, 1)
                .expect("quarter start is always valid")
        }
        TemporalGrain::Year => {
            NaiveDate::from_ymd_opt(date.year(), 1, 1).expect("Jan 1 is always valid")
        }
    }
}

/// Returns the start of the period immediately after the period containing the given date.
///
/// # Examples
///
/// ```
/// use chrono::NaiveDate;
/// use fraiseql_core::compiler::fact_table::TemporalGrain;
/// use fraiseql_core::runtime::partial_period::next_period_start;
///
/// let d = NaiveDate::from_ymd_opt(2024, 12, 15).unwrap();
/// assert_eq!(
///     next_period_start(d, TemporalGrain::Month),
///     NaiveDate::from_ymd_opt(2025, 1, 1).unwrap()
/// );
/// ```
///
/// # Panics
///
/// Panics if the computed date is outside the `NaiveDate` range, which cannot
/// happen for practical calendar dates (years 0–9999).
#[must_use]
pub fn next_period_start(date: NaiveDate, grain: TemporalGrain) -> NaiveDate {
    let start = period_start(date, grain);
    match grain {
        TemporalGrain::Day => start + TimeDelta::days(1),
        TemporalGrain::Week => start + TimeDelta::weeks(1),
        TemporalGrain::Month => {
            if start.month() == 12 {
                NaiveDate::from_ymd_opt(start.year() + 1, 1, 1)
                    .expect("next year Jan 1 is valid")
            } else {
                NaiveDate::from_ymd_opt(start.year(), start.month() + 1, 1)
                    .expect("next month day 1 is valid")
            }
        }
        TemporalGrain::Quarter => {
            if start.month() == 10 {
                NaiveDate::from_ymd_opt(start.year() + 1, 1, 1)
                    .expect("next year Q1 start is valid")
            } else {
                NaiveDate::from_ymd_opt(start.year(), start.month() + 3, 1)
                    .expect("next quarter start is valid")
            }
        }
        TemporalGrain::Year => {
            NaiveDate::from_ymd_opt(start.year() + 1, 1, 1).expect("next year Jan 1 is valid")
        }
    }
}

/// Plan describing which UNION ALL branches to generate.
///
/// A partial-period UNION ALL query has up to 3 branches:
/// - **`partial_leading`**: fine-grain rows from the non-aligned lower bound to the
///   next period boundary (omitted when the lower bound is period-aligned).
/// - **`complete_middle`**: coarse-grain rows for fully completed periods between
///   the leading partial period and the current period (omitted when there are no
///   complete periods in range).
/// - **`current_period`**: fine-grain rows for the in-progress period up to today
///   (always present).
///
/// Date ranges are half-open intervals: `[gte, lt)`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BranchPlan {
    /// Partial leading period: `[gte, lt)`. `None` when the lower bound is period-aligned.
    pub partial_leading: Option<(NaiveDate, NaiveDate)>,
    /// Complete middle periods: `[gte, lt)`. `None` when no complete periods exist.
    pub complete_middle: Option<(NaiveDate, NaiveDate)>,
    /// Current in-progress period: `[gte, lt)`. Always present.
    pub current_period:  (NaiveDate, NaiveDate),
}

/// Computes which UNION ALL branches are needed given a lower bound, grain, and today's date.
///
/// # Arguments
///
/// * `lower_bound` — the effective inclusive lower-bound date extracted from the WHERE clause
/// * `grain` — the temporal granularity of the coarse-grain view
/// * `today` — today's date (injected for deterministic testing)
///
/// # Examples
///
/// ```
/// use chrono::NaiveDate;
/// use fraiseql_core::compiler::fact_table::TemporalGrain;
/// use fraiseql_core::runtime::partial_period::determine_branches;
///
/// let lower = NaiveDate::from_ymd_opt(2024, 1, 15).unwrap();
/// let today = NaiveDate::from_ymd_opt(2024, 3, 20).unwrap();
/// let plan = determine_branches(lower, TemporalGrain::Month, today);
///
/// // B1: Jan 15 – Feb 1 (partial leading)
/// assert_eq!(plan.partial_leading, Some((
///     NaiveDate::from_ymd_opt(2024, 1, 15).unwrap(),
///     NaiveDate::from_ymd_opt(2024, 2, 1).unwrap(),
/// )));
/// // B2: Feb 1 – Mar 1 (complete middle)
/// assert_eq!(plan.complete_middle, Some((
///     NaiveDate::from_ymd_opt(2024, 2, 1).unwrap(),
///     NaiveDate::from_ymd_opt(2024, 3, 1).unwrap(),
/// )));
/// // B3: Mar 1 – Mar 21 (current period, today+1 exclusive)
/// assert_eq!(plan.current_period, (
///     NaiveDate::from_ymd_opt(2024, 3, 1).unwrap(),
///     NaiveDate::from_ymd_opt(2024, 3, 21).unwrap(),
/// ));
/// ```
#[must_use]
pub fn determine_branches(
    lower_bound: NaiveDate,
    grain: TemporalGrain,
    today: NaiveDate,
) -> BranchPlan {
    let aligned = is_period_aligned(lower_bound, grain);
    let next_ps = next_period_start(lower_bound, grain);
    let current_ps = period_start(today, grain);

    // B2 starts at lower_bound when aligned, else at next_ps
    let b2_start = if aligned { lower_bound } else { next_ps };
    let include_b2 = b2_start < current_ps;

    // B3 upper bound: exclusive tomorrow makes "date < tomorrow" ≡ "date <= today"
    let today_exclusive = today + TimeDelta::days(1);

    BranchPlan {
        partial_leading: if aligned {
            None
        } else {
            Some((lower_bound, next_ps))
        },
        complete_middle: if include_b2 {
            Some((b2_start, current_ps))
        } else {
            None
        },
        current_period:  (current_ps, today_exclusive),
    }
}

#[cfg(test)]
#[path = "partial_period_tests.rs"]
mod tests;
