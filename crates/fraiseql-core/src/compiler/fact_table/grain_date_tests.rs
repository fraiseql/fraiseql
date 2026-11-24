//! A bound on the grain column is read as the `Date` scalar publishes it, `YYYY-MM-DD`,
//! and its period arithmetic holds to the last day of the calendar, 9999-12-31.

#![allow(clippy::unwrap_used, clippy::panic)] // Reason: test code, panics acceptable

use serde_json::json;

use super::*;

fn grain(bucket: GrainBucket) -> FactGrain {
    FactGrain {
        column: "day".to_string(),
        bucket,
        edges_from: None,
    }
}

/// The message `key: on` is refused with, or `None` when it is answered.
fn bound(bucket: GrainBucket, key: &str, on: &str) -> Option<String> {
    let operator = match key.rsplit('_').next().unwrap() {
        "gte" => WhereOperator::Gte,
        "gt" => WhereOperator::Gt,
        "lt" => WhereOperator::Lt,
        "lte" => WhereOperator::Lte,
        other => panic!("{other}"),
    };
    grain(bucket)
        .check_bound("tf_t", key, &operator, &json!(on))
        .err()
        .map(|e| e.to_string())
}

#[test]
fn a_bound_on_each_periods_first_day_is_answered() {
    for (bucket, on) in [
        (GrainBucket::Week, "2024-01-01"),
        (GrainBucket::HalfMonth, "2024-01-16"),
        (GrainBucket::Month, "2024-01-01"),
        (GrainBucket::Quarter, "2024-04-01"),
        (GrainBucket::Semester, "2024-07-01"),
        (GrainBucket::Year, "2024-01-01"),
    ] {
        assert_eq!(bound(bucket, "day_gte", on), None, "{bucket:?} from {on}");
    }
}

#[test]
fn a_bound_on_each_periods_last_day_is_answered() {
    for (bucket, on) in [
        (GrainBucket::Week, "2024-01-07"),
        (GrainBucket::HalfMonth, "2024-02-15"),
        (GrainBucket::HalfMonth, "2024-02-29"),
        (GrainBucket::Month, "2024-02-29"),
        (GrainBucket::Quarter, "2024-03-31"),
        (GrainBucket::Semester, "2024-12-31"),
        (GrainBucket::Year, "2023-12-31"),
    ] {
        assert_eq!(bound(bucket, "day_lte", on), None, "{bucket:?} through {on}");
    }
}

/// Forms a lenient date reader took, which the column's operand check refuses anyway: an
/// unpadded field, a sign, leading space, a year past 9999; and forms ISO 8601 has that the
/// `Date` scalar does not: basic, extended-year and with a time of day.
#[test]
fn a_bound_not_written_yyyy_mm_dd_is_refused_as_not_a_date() {
    for on in [
        "2024-1-5",
        "+2024-01-15",
        " 2024-01-15",
        "+10000-01-01",
        "2024-02-30",
        "20240115",
        "+002024-01-15",
        "2024-01-15T00:00:00",
        "2024-01-15 00:00",
    ] {
        let refused = bound(GrainBucket::Month, "day_gte", on).unwrap_or_default();
        assert!(
            refused.contains("`day_gte` takes a date (`YYYY-MM-DD`), got"),
            "{on:?}: {refused}"
        );
    }
}

/// 9999-12-31 ends every period but its week: the day after it would start a month, a
/// quarter and a year, and be a Saturday. No bound past the calendar is offered, as the
/// column's operand check would refuse it.
#[test]
fn a_bound_through_the_calendars_last_day_ends_every_period_but_a_week() {
    for bucket in [
        GrainBucket::HalfMonth,
        GrainBucket::Month,
        GrainBucket::Quarter,
        GrainBucket::Semester,
        GrainBucket::Year,
    ] {
        assert_eq!(bound(bucket, "day_lte", "9999-12-31"), None, "{bucket:?}");
    }
    let refused = bound(GrainBucket::Week, "day_lte", "9999-12-31").unwrap();
    assert!(
        refused.contains(
            "`day_lte: 9999-12-31` does not end a week, and a week's row cannot be split. \
             Bound the range where a week ends, such as `day_lte: 9999-12-26`"
        ),
        "{refused}"
    );
    assert!(!refused.contains("+10000"), "no bound past the calendar is offered: {refused}");
}

/// The calendar has no period after its last: a bound inside it names only the start.
#[test]
fn a_bound_inside_the_calendars_last_period_names_its_start() {
    let refused = bound(GrainBucket::Month, "day_gte", "9999-12-15").unwrap();
    assert!(
        refused.ends_with("Bound the range where a month starts, such as `day_gte: 9999-12-01`"),
        "{refused}"
    );
}

/// A period starting before year 0 is named with a signed year: 0000-01-01 is a Saturday,
/// and its week starts on Monday -0001-12-27.
#[test]
fn a_boundary_before_year_zero_is_written_with_a_signed_year() {
    let refused = bound(GrainBucket::Week, "day_gte", "0000-01-01").unwrap();
    assert!(
        refused.ends_with("such as `day_gte: -0001-12-27` or `day_gte: 0000-01-03`"),
        "{refused}"
    );
}
