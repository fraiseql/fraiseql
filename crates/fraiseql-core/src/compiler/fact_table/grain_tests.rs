#![allow(clippy::unwrap_used, clippy::panic)] // Reason: test code, panics acceptable
use std::collections::HashMap;

use chrono::NaiveDate;
use serde_json::json;

use super::*;
use crate::compiler::fact_table::{
    CalendarDimension, DimensionColumn, FactRowScope, FactTableMetadata, FilterColumn, SqlType,
};

fn day(text: &str) -> NaiveDate {
    NaiveDate::parse_from_str(text, "%Y-%m-%d").unwrap()
}

fn grain(bucket: GrainBucket) -> FactGrain {
    FactGrain {
        column: "day".to_string(),
        bucket,
    }
}

/// The message `key: value` is refused with, or `None` when it is answered.
fn bound(bucket: GrainBucket, key: &str, value: &serde_json::Value) -> Option<String> {
    let operator = match key.rsplit('_').next().unwrap() {
        "gte" => WhereOperator::Gte,
        "gt" => WhereOperator::Gt,
        "lt" => WhereOperator::Lt,
        "lte" => WhereOperator::Lte,
        "eq" => WhereOperator::Eq,
        "isnull" => WhereOperator::IsNull,
        other => panic!("{other}"),
    };
    grain(bucket)
        .check_bound("tf_t", key, &operator, value)
        .err()
        .map(|e| e.to_string())
}

#[test]
fn each_period_starts_where_postgresql_starts_it() {
    for (bucket, on, start) in [
        // 1 January 2026 is a Thursday: its ISO week starts on Monday 29 December.
        (GrainBucket::Week, "2026-01-01", "2025-12-29"),
        (GrainBucket::Week, "2026-01-05", "2026-01-05"),
        (GrainBucket::HalfMonth, "2026-02-15", "2026-02-01"),
        (GrainBucket::HalfMonth, "2026-02-16", "2026-02-16"),
        (GrainBucket::Month, "2024-02-29", "2024-02-01"),
        (GrainBucket::Quarter, "2026-05-20", "2026-04-01"),
        (GrainBucket::Quarter, "2026-12-31", "2026-10-01"),
        (GrainBucket::Semester, "2026-06-30", "2026-01-01"),
        (GrainBucket::Semester, "2026-07-01", "2026-07-01"),
        (GrainBucket::Year, "2026-12-31", "2026-01-01"),
    ] {
        assert_eq!(bucket.start(day(on)), day(start), "{bucket:?} of {on}");
    }
}

#[test]
fn a_bound_on_a_period_boundary_is_answered() {
    for (bucket, key, on) in [
        (GrainBucket::Week, "day_gte", "2026-01-05"),
        (GrainBucket::Week, "day_lte", "2026-01-04"),
        (GrainBucket::HalfMonth, "day_gt", "2026-01-15"),
        (GrainBucket::HalfMonth, "day_lt", "2026-01-16"),
        (GrainBucket::Quarter, "day_gte", "2026-10-01"),
        (GrainBucket::Semester, "day_lt", "2026-07-01"),
        (GrainBucket::Semester, "day_lte", "2026-06-30"),
        (GrainBucket::Year, "day_lte", "2026-12-31"),
        (GrainBucket::Year, "day_gt", "2025-12-31"),
    ] {
        assert_eq!(bound(bucket, key, &json!(on)), None, "{bucket:?} {key} {on}");
    }
}

#[test]
fn a_bound_inside_a_period_names_the_boundaries_on_either_side() {
    for (bucket, key, on, before, after) in [
        (GrainBucket::Week, "day_gte", "2026-01-01", "2025-12-29", "2026-01-05"),
        (GrainBucket::HalfMonth, "day_gte", "2026-01-20", "2026-01-16", "2026-02-01"),
        (GrainBucket::HalfMonth, "day_gte", "2026-01-10", "2026-01-01", "2026-01-16"),
        (GrainBucket::Quarter, "day_lt", "2026-05-20", "2026-04-01", "2026-07-01"),
        (GrainBucket::Semester, "day_lte", "2026-03-31", "2025-12-31", "2026-06-30"),
        (GrainBucket::Year, "day_gt", "2026-06-30", "2025-12-31", "2026-12-31"),
    ] {
        let message = bound(bucket, key, &json!(on)).unwrap_or_else(|| panic!("{key} {on}"));
        for shown in [before, after] {
            assert!(message.contains(&format!("`{key}: {shown}`")), "{message}");
        }
    }
}

#[test]
fn the_refusal_says_what_the_table_holds_and_what_it_answers() {
    assert_eq!(
        bound(GrainBucket::Month, "day_lte", &json!("2026-03-20")).unwrap(),
        "Validation error: fact table `tf_t` holds one row per month of `day`: `day_lte: \
         2026-03-20` does not end a month, and a month's row cannot be split. Bound the range \
         where a month ends, such as `day_lte: 2026-02-28` or `day_lte: 2026-03-31`"
    );
}

#[test]
fn a_day_grain_refuses_no_bound() {
    for key in ["day_gte", "day_lte", "day_eq"] {
        assert_eq!(bound(GrainBucket::Day, key, &json!("2026-03-20")), None, "{key}");
    }
}

#[test]
fn a_null_test_reads_no_date_and_a_non_date_is_refused() {
    assert_eq!(bound(GrainBucket::Month, "day_isnull", &json!(true)), None);
    let message = bound(GrainBucket::Month, "day_gte", &json!(20_260_101)).unwrap();
    assert!(message.contains("`day_gte` takes a date"), "{message}");
}

#[test]
fn a_bucket_is_served_only_when_a_row_falls_wholly_inside_it() {
    let served = |g: GrainBucket| g.served().iter().map(|b| b.name()).collect::<Vec<_>>();
    assert_eq!(served(GrainBucket::Week), ["week"]);
    assert_eq!(
        served(GrainBucket::HalfMonth),
        ["half_month", "month", "quarter", "semester", "year"]
    );
    assert_eq!(served(GrainBucket::Quarter), ["quarter", "semester", "year"]);
    assert_eq!(served(GrainBucket::Year), ["year"]);
    assert!(GrainBucket::Day.serves(TemporalBucket::Week));
    assert!(!GrainBucket::Week.serves(TemporalBucket::Month));
    assert!(!GrainBucket::Month.serves(TemporalBucket::Hour));
    let message = grain(GrainBucket::Semester)
        .check_bucket("tf_t", "day_quarter", TemporalBucket::Quarter)
        .unwrap_err()
        .to_string();
    assert!(message.contains("Group by `day_semester`, `day_year`"), "{message}");
}

#[test]
fn a_grain_reads_its_published_shape_and_refuses_any_other() {
    let read: FactGrain =
        serde_json::from_value(json!({ "column": "day", "bucket": "half_month" })).unwrap();
    assert_eq!(read, grain(GrainBucket::HalfMonth));
    assert_eq!(
        serde_json::to_value(&read).unwrap(),
        json!({ "column": "day", "bucket": "half_month" })
    );
    for refused in [
        json!({ "column": "day", "bucket": "fortnight" }),
        json!({ "column": "day", "bucket": "hour" }),
        json!({ "column": "day", "bucket": "HalfMonth" }),
        json!({ "column": "day", "bucket": "month", "over": "day" }),
        json!({ "column": "day" }),
    ] {
        assert!(serde_json::from_value::<FactGrain>(refused.clone()).is_err(), "{refused}");
    }
}

fn table(grain: Option<FactGrain>, filters: &[(&str, SqlType)]) -> FactTableMetadata {
    FactTableMetadata {
        table_name: "tf_t".to_string(),
        type_name: None,
        row_scope: FactRowScope::Server,
        inject_params: indexmap::IndexMap::default(),
        measures: vec![],
        dimensions: DimensionColumn {
            name:  "data".to_string(),
            paths: vec![],
        },
        denormalized_filters: filters
            .iter()
            .map(|(name, sql_type)| FilterColumn {
                name:      (*name).to_string(),
                sql_type:  sql_type.clone(),
                indexed:   true,
                hierarchy: None,
            })
            .collect(),
        calendar_dimensions: vec![],
        grain,
        native_dimension_mapping: HashMap::new(),
    }
}

#[test]
fn a_grain_on_a_declared_date_column_loads() {
    let month = Some(grain(GrainBucket::Month));
    assert!(table(month, &[("day", SqlType::Date)]).grain_violations().is_empty());
    assert!(table(None, &[]).grain_violations().is_empty());
}

#[test]
fn a_grain_on_a_column_it_cannot_check_is_refused_at_load() {
    let month = || Some(grain(GrainBucket::Month));
    for (filters, expected) in [
        (vec![], "`grain.column` `day` is not one of its `denormalized_filters`"),
        (vec![("day", SqlType::TimestampTz)], "`grain` is for date columns only"),
        (vec![("day", SqlType::LocalTimestamp)], "`grain` is for date columns only"),
    ] {
        let violations = table(month(), &filters).grain_violations();
        assert_eq!(violations.len(), 1, "{violations:?}");
        assert!(violations[0].contains(expected), "{violations:?}");
    }
    let mut calendar = table(month(), &[("day", SqlType::Date)]);
    calendar.calendar_dimensions = vec![CalendarDimension {
        source_column: "day".to_string(),
        granularities: vec![],
    }];
    let violations = calendar.grain_violations();
    assert!(
        violations.len() == 1 && violations[0].contains("a calendar dimension reads `day`"),
        "{violations:?}"
    );
}
