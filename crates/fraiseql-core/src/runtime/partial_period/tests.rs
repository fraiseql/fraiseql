#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

use chrono::NaiveDate;
use fraiseql_db::{WhereClause, WhereOperator};
use serde_json::json;

use super::*;

fn date(y: i32, m: u32, d: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(y, m, d).unwrap()
}

// =============================================================================
// is_period_aligned
// =============================================================================

#[test]
fn test_day_always_aligned() {
    assert!(is_period_aligned(date(2024, 3, 15), TemporalGrain::Day));
    assert!(is_period_aligned(date(2024, 1, 1), TemporalGrain::Day));
}

#[test]
fn test_week_alignment() {
    // 2024-01-01 is Monday
    assert!(is_period_aligned(date(2024, 1, 1), TemporalGrain::Week));
    // 2024-01-02 is Tuesday
    assert!(!is_period_aligned(date(2024, 1, 2), TemporalGrain::Week));
    // 2024-01-08 is Monday
    assert!(is_period_aligned(date(2024, 1, 8), TemporalGrain::Week));
    // Sunday
    assert!(!is_period_aligned(date(2024, 1, 7), TemporalGrain::Week));
}

#[test]
fn test_month_alignment() {
    assert!(is_period_aligned(date(2024, 1, 1), TemporalGrain::Month));
    assert!(is_period_aligned(date(2024, 2, 1), TemporalGrain::Month));
    assert!(!is_period_aligned(date(2024, 1, 15), TemporalGrain::Month));
    assert!(!is_period_aligned(date(2024, 12, 31), TemporalGrain::Month));
}

#[test]
fn test_quarter_alignment() {
    assert!(is_period_aligned(date(2024, 1, 1), TemporalGrain::Quarter));
    assert!(is_period_aligned(date(2024, 4, 1), TemporalGrain::Quarter));
    assert!(is_period_aligned(date(2024, 7, 1), TemporalGrain::Quarter));
    assert!(is_period_aligned(date(2024, 10, 1), TemporalGrain::Quarter));
    // Not quarter starts
    assert!(!is_period_aligned(date(2024, 2, 1), TemporalGrain::Quarter));
    assert!(!is_period_aligned(date(2024, 3, 1), TemporalGrain::Quarter));
    assert!(!is_period_aligned(date(2024, 5, 15), TemporalGrain::Quarter));
}

#[test]
fn test_year_alignment() {
    assert!(is_period_aligned(date(2024, 1, 1), TemporalGrain::Year));
    assert!(is_period_aligned(date(2025, 1, 1), TemporalGrain::Year));
    assert!(!is_period_aligned(date(2024, 1, 2), TemporalGrain::Year));
    assert!(!is_period_aligned(date(2024, 6, 1), TemporalGrain::Year));
}

// =============================================================================
// period_start
// =============================================================================

#[test]
fn test_period_start_day() {
    assert_eq!(period_start(date(2024, 3, 15), TemporalGrain::Day), date(2024, 3, 15));
}

#[test]
fn test_period_start_week() {
    // 2024-01-03 is Wednesday, week starts Monday 2024-01-01
    assert_eq!(period_start(date(2024, 1, 3), TemporalGrain::Week), date(2024, 1, 1));
    // Already Monday
    assert_eq!(period_start(date(2024, 1, 1), TemporalGrain::Week), date(2024, 1, 1));
    // Sunday → previous Monday
    assert_eq!(period_start(date(2024, 1, 7), TemporalGrain::Week), date(2024, 1, 1));
}

#[test]
fn test_period_start_month() {
    assert_eq!(period_start(date(2024, 2, 15), TemporalGrain::Month), date(2024, 2, 1));
    assert_eq!(period_start(date(2024, 2, 1), TemporalGrain::Month), date(2024, 2, 1));
    assert_eq!(period_start(date(2024, 12, 31), TemporalGrain::Month), date(2024, 12, 1));
}

#[test]
fn test_period_start_quarter() {
    assert_eq!(period_start(date(2024, 5, 20), TemporalGrain::Quarter), date(2024, 4, 1));
    assert_eq!(period_start(date(2024, 1, 1), TemporalGrain::Quarter), date(2024, 1, 1));
    assert_eq!(period_start(date(2024, 3, 31), TemporalGrain::Quarter), date(2024, 1, 1));
    assert_eq!(period_start(date(2024, 12, 15), TemporalGrain::Quarter), date(2024, 10, 1));
}

#[test]
fn test_period_start_year() {
    assert_eq!(period_start(date(2024, 3, 15), TemporalGrain::Year), date(2024, 1, 1));
    assert_eq!(period_start(date(2024, 1, 1), TemporalGrain::Year), date(2024, 1, 1));
}

// =============================================================================
// next_period_start
// =============================================================================

#[test]
fn test_next_period_start_day() {
    assert_eq!(next_period_start(date(2024, 1, 15), TemporalGrain::Day), date(2024, 1, 16));
}

#[test]
fn test_next_period_start_week() {
    // Wednesday → next Monday
    assert_eq!(next_period_start(date(2024, 1, 3), TemporalGrain::Week), date(2024, 1, 8));
}

#[test]
fn test_next_period_start_month() {
    assert_eq!(next_period_start(date(2024, 1, 15), TemporalGrain::Month), date(2024, 2, 1));
    // Year boundary
    assert_eq!(next_period_start(date(2024, 12, 1), TemporalGrain::Month), date(2025, 1, 1));
    // Leap year February
    assert_eq!(next_period_start(date(2024, 2, 29), TemporalGrain::Month), date(2024, 3, 1));
}

#[test]
fn test_next_period_start_quarter() {
    assert_eq!(next_period_start(date(2024, 2, 15), TemporalGrain::Quarter), date(2024, 4, 1));
    // Q4 → next year Q1
    assert_eq!(next_period_start(date(2024, 10, 1), TemporalGrain::Quarter), date(2025, 1, 1));
}

#[test]
fn test_next_period_start_year() {
    assert_eq!(next_period_start(date(2024, 6, 15), TemporalGrain::Year), date(2025, 1, 1));
}

// =============================================================================
// determine_branches
// =============================================================================

#[test]
fn test_three_branch_mid_month() {
    // Lower bound mid-January, today in March
    let plan = determine_branches(date(2024, 1, 15), TemporalGrain::Month, date(2024, 3, 20));

    assert_eq!(plan.partial_leading, Some((date(2024, 1, 15), date(2024, 2, 1))));
    assert_eq!(plan.complete_middle, Some((date(2024, 2, 1), date(2024, 3, 1))));
    assert_eq!(plan.current_period, (date(2024, 3, 1), date(2024, 3, 21)));
}

#[test]
fn test_two_branch_aligned_lower_bound() {
    // Lower bound exactly on month boundary → no B1
    let plan = determine_branches(date(2024, 2, 1), TemporalGrain::Month, date(2024, 4, 10));

    assert_eq!(plan.partial_leading, None);
    assert_eq!(plan.complete_middle, Some((date(2024, 2, 1), date(2024, 4, 1))));
    assert_eq!(plan.current_period, (date(2024, 4, 1), date(2024, 4, 11)));
}

#[test]
fn test_two_branch_lower_in_current_period() {
    // Lower bound in the current period → B1 + B3, no B2
    let plan = determine_branches(date(2024, 3, 15), TemporalGrain::Month, date(2024, 3, 20));

    assert_eq!(plan.partial_leading, Some((date(2024, 3, 15), date(2024, 4, 1))));
    assert_eq!(plan.complete_middle, None);
    assert_eq!(plan.current_period, (date(2024, 3, 1), date(2024, 3, 21)));
}

#[test]
fn test_single_branch_aligned_same_period() {
    // Lower bound is period-aligned AND in the current period → only B3
    let plan = determine_branches(date(2024, 3, 1), TemporalGrain::Month, date(2024, 3, 20));

    assert_eq!(plan.partial_leading, None);
    assert_eq!(plan.complete_middle, None);
    assert_eq!(plan.current_period, (date(2024, 3, 1), date(2024, 3, 21)));
}

#[test]
fn test_quarter_grain_three_branches() {
    // Mid-Q1 lower bound, today in Q4
    let plan = determine_branches(date(2024, 2, 15), TemporalGrain::Quarter, date(2024, 11, 10));

    assert_eq!(plan.partial_leading, Some((date(2024, 2, 15), date(2024, 4, 1))));
    assert_eq!(plan.complete_middle, Some((date(2024, 4, 1), date(2024, 10, 1))));
    assert_eq!(plan.current_period, (date(2024, 10, 1), date(2024, 11, 11)));
}

#[test]
fn test_year_grain() {
    let plan = determine_branches(date(2023, 6, 15), TemporalGrain::Year, date(2025, 3, 10));

    assert_eq!(plan.partial_leading, Some((date(2023, 6, 15), date(2024, 1, 1))));
    assert_eq!(plan.complete_middle, Some((date(2024, 1, 1), date(2025, 1, 1))));
    assert_eq!(plan.current_period, (date(2025, 1, 1), date(2025, 3, 11)));
}

#[test]
fn test_week_grain() {
    // Wednesday lower bound
    let plan = determine_branches(date(2024, 1, 3), TemporalGrain::Week, date(2024, 1, 17));

    // B1: Wed Jan 3 – Mon Jan 8
    assert_eq!(plan.partial_leading, Some((date(2024, 1, 3), date(2024, 1, 8))));
    // B2: Mon Jan 8 – Mon Jan 15
    assert_eq!(plan.complete_middle, Some((date(2024, 1, 8), date(2024, 1, 15))));
    // B3: Mon Jan 15 – Thu Jan 18
    assert_eq!(plan.current_period, (date(2024, 1, 15), date(2024, 1, 18)));
}

#[test]
fn test_lower_bound_equals_today() {
    let plan = determine_branches(date(2024, 3, 15), TemporalGrain::Month, date(2024, 3, 15));

    // B1 exists: Mar 15 – Apr 1
    assert_eq!(plan.partial_leading, Some((date(2024, 3, 15), date(2024, 4, 1))));
    assert_eq!(plan.complete_middle, None);
    // B3: Mar 1 – Mar 16
    assert_eq!(plan.current_period, (date(2024, 3, 1), date(2024, 3, 16)));
}

// =============================================================================
// extract_lower_date_bound
// =============================================================================

#[test]
fn test_extract_gte_simple() {
    let wc = WhereClause::Field {
        path: vec!["date".into()],
        operator: WhereOperator::Gte,
        value: json!("2024-01-15"),
    };
    assert_eq!(extract_lower_date_bound(&wc, "date"), Some(date(2024, 1, 15)));
}

#[test]
fn test_extract_gt_converts_to_next_day() {
    let wc = WhereClause::Field {
        path: vec!["date".into()],
        operator: WhereOperator::Gt,
        value: json!("2024-01-14"),
    };
    // gt 14th = gte 15th
    assert_eq!(extract_lower_date_bound(&wc, "date"), Some(date(2024, 1, 15)));
}

#[test]
fn test_extract_from_and_chain() {
    let wc = WhereClause::And(vec![
        WhereClause::Field {
            path: vec!["tenant_id".into()],
            operator: WhereOperator::Eq,
            value: json!("t1"),
        },
        WhereClause::Field {
            path: vec!["date".into()],
            operator: WhereOperator::Gte,
            value: json!("2024-01-15"),
        },
    ]);
    assert_eq!(extract_lower_date_bound(&wc, "date"), Some(date(2024, 1, 15)));
}

#[test]
fn test_extract_or_returns_none() {
    let wc = WhereClause::Or(vec![
        WhereClause::Field {
            path: vec!["date".into()],
            operator: WhereOperator::Gte,
            value: json!("2024-01-15"),
        },
        WhereClause::Field {
            path: vec!["status".into()],
            operator: WhereOperator::Eq,
            value: json!("active"),
        },
    ]);
    assert!(extract_lower_date_bound(&wc, "date").is_none());
}

#[test]
fn test_extract_not_returns_none() {
    let wc = WhereClause::Not(Box::new(WhereClause::Field {
        path: vec!["date".into()],
        operator: WhereOperator::Lt,
        value: json!("2024-01-15"),
    }));
    assert!(extract_lower_date_bound(&wc, "date").is_none());
}

#[test]
fn test_extract_no_date_column_returns_none() {
    let wc = WhereClause::Field {
        path: vec!["status".into()],
        operator: WhereOperator::Eq,
        value: json!("active"),
    };
    assert!(extract_lower_date_bound(&wc, "date").is_none());
}

#[test]
fn test_extract_wrong_operator_returns_none() {
    let wc = WhereClause::Field {
        path: vec!["date".into()],
        operator: WhereOperator::Lt,
        value: json!("2024-01-15"),
    };
    assert!(extract_lower_date_bound(&wc, "date").is_none());
}

#[test]
fn test_extract_native_field_gte() {
    let wc = WhereClause::NativeField {
        column: "period_start".into(),
        pg_cast: "date".into(),
        operator: WhereOperator::Gte,
        value: json!("2024-03-01"),
    };
    assert_eq!(extract_lower_date_bound(&wc, "period_start"), Some(date(2024, 3, 1)));
}

#[test]
fn test_extract_native_field_wrong_column() {
    let wc = WhereClause::NativeField {
        column: "created_at".into(),
        pg_cast: "date".into(),
        operator: WhereOperator::Gte,
        value: json!("2024-03-01"),
    };
    assert!(extract_lower_date_bound(&wc, "period_start").is_none());
}

// =============================================================================
// split_where_clause
// =============================================================================

#[test]
fn test_split_single_date_condition() {
    let wc = WhereClause::Field {
        path: vec!["date".into()],
        operator: WhereOperator::Gte,
        value: json!("2024-01-15"),
    };
    let result = split_where_clause(&wc, "date").unwrap();
    assert_eq!(result.lower_bound, date(2024, 1, 15));
    assert_eq!(result.remaining, None);
}

#[test]
fn test_split_and_chain_three_conditions() {
    let wc = WhereClause::And(vec![
        WhereClause::Field {
            path: vec!["tenant_id".into()],
            operator: WhereOperator::Eq,
            value: json!("t1"),
        },
        WhereClause::Field {
            path: vec!["date".into()],
            operator: WhereOperator::Gte,
            value: json!("2024-01-15"),
        },
        WhereClause::Field {
            path: vec!["status".into()],
            operator: WhereOperator::Eq,
            value: json!("active"),
        },
    ]);
    let result = split_where_clause(&wc, "date").unwrap();
    assert_eq!(result.lower_bound, date(2024, 1, 15));
    assert_eq!(
        result.remaining,
        Some(WhereClause::And(vec![
            WhereClause::Field {
                path: vec!["tenant_id".into()],
                operator: WhereOperator::Eq,
                value: json!("t1"),
            },
            WhereClause::Field {
                path: vec!["status".into()],
                operator: WhereOperator::Eq,
                value: json!("active"),
            },
        ]))
    );
}

#[test]
fn test_split_and_chain_two_conditions_unwraps() {
    let wc = WhereClause::And(vec![
        WhereClause::Field {
            path: vec!["tenant_id".into()],
            operator: WhereOperator::Eq,
            value: json!("t1"),
        },
        WhereClause::Field {
            path: vec!["date".into()],
            operator: WhereOperator::Gte,
            value: json!("2024-01-15"),
        },
    ]);
    let result = split_where_clause(&wc, "date").unwrap();
    assert_eq!(result.lower_bound, date(2024, 1, 15));
    // Single remaining child: AND wrapper is unwrapped
    assert_eq!(
        result.remaining,
        Some(WhereClause::Field {
            path: vec!["tenant_id".into()],
            operator: WhereOperator::Eq,
            value: json!("t1"),
        })
    );
}

#[test]
fn test_split_no_match_returns_none() {
    let wc = WhereClause::Field {
        path: vec!["status".into()],
        operator: WhereOperator::Eq,
        value: json!("active"),
    };
    assert!(split_where_clause(&wc, "date").is_none());
}

#[test]
fn test_split_or_returns_none() {
    let wc = WhereClause::Or(vec![
        WhereClause::Field {
            path: vec!["date".into()],
            operator: WhereOperator::Gte,
            value: json!("2024-01-15"),
        },
        WhereClause::Field {
            path: vec!["status".into()],
            operator: WhereOperator::Eq,
            value: json!("active"),
        },
    ]);
    assert!(split_where_clause(&wc, "date").is_none());
}

#[test]
fn test_split_native_field() {
    let wc = WhereClause::NativeField {
        column: "period_start".into(),
        pg_cast: "date".into(),
        operator: WhereOperator::Gte,
        value: json!("2024-03-01"),
    };
    let result = split_where_clause(&wc, "period_start").unwrap();
    assert_eq!(result.lower_bound, date(2024, 3, 1));
    assert_eq!(result.remaining, None);
}

#[test]
fn test_split_gt_conversion() {
    let wc = WhereClause::And(vec![
        WhereClause::Field {
            path: vec!["date".into()],
            operator: WhereOperator::Gt,
            value: json!("2024-01-14"),
        },
        WhereClause::Field {
            path: vec!["status".into()],
            operator: WhereOperator::Eq,
            value: json!("active"),
        },
    ]);
    let result = split_where_clause(&wc, "date").unwrap();
    // gt 14th → gte 15th
    assert_eq!(result.lower_bound, date(2024, 1, 15));
    assert_eq!(
        result.remaining,
        Some(WhereClause::Field {
            path: vec!["status".into()],
            operator: WhereOperator::Eq,
            value: json!("active"),
        })
    );
}

// =============================================================================
// should_use_partial_period
// =============================================================================

mod should_use_tests {
    use super::*;
    use crate::compiler::fact_table::{
        DimensionColumn, FactTableMetadata, MeasureColumn, PartialPeriodConfig, SqlType,
        TemporalGrain,
    };

    fn metadata_with_pp() -> FactTableMetadata {
        FactTableMetadata {
            table_name: "v_events_month".into(),
            measures: vec![MeasureColumn {
                name: "volume".into(),
                sql_type: SqlType::BigInt,
                nullable: false,
            }],
            dimensions: DimensionColumn {
                name: "data".into(),
                paths: vec![],
            },
            denormalized_filters: vec![],
            calendar_dimensions: vec![],
            partial_period: Some(PartialPeriodConfig {
                fine_grain_view: "v_events_day".into(),
                time_grain_column: "period_start".into(),
                time_grain_trunc: TemporalGrain::Month,
            }),
            native_measures: std::collections::HashMap::new(),
            native_dimension_mapping: std::collections::HashMap::new(),
        }
    }

    fn metadata_without_pp() -> FactTableMetadata {
        let mut m = metadata_with_pp();
        m.partial_period = None;
        m
    }

    #[test]
    fn test_triggers_when_conditions_met() {
        let m = metadata_with_pp();
        let wc = WhereClause::Field {
            path: vec!["period_start".into()],
            operator: WhereOperator::Gte,
            value: json!("2024-01-15"),
        };
        let today = date(2024, 3, 20);
        let result = should_use_partial_period(&m, Some(&wc), today);
        assert!(result.is_some());
        let (lower, config) = result.unwrap();
        assert_eq!(lower, date(2024, 1, 15));
        assert_eq!(config.fine_grain_view, "v_events_day");
    }

    #[test]
    fn test_none_without_partial_period_config() {
        let m = metadata_without_pp();
        let wc = WhereClause::Field {
            path: vec!["period_start".into()],
            operator: WhereOperator::Gte,
            value: json!("2024-01-15"),
        };
        assert!(should_use_partial_period(&m, Some(&wc), date(2024, 3, 20)).is_none());
    }

    #[test]
    fn test_none_without_where_clause() {
        let m = metadata_with_pp();
        assert!(should_use_partial_period(&m, None, date(2024, 3, 20)).is_none());
    }

    #[test]
    fn test_none_when_no_date_condition() {
        let m = metadata_with_pp();
        let wc = WhereClause::Field {
            path: vec!["status".into()],
            operator: WhereOperator::Eq,
            value: json!("active"),
        };
        assert!(should_use_partial_period(&m, Some(&wc), date(2024, 3, 20)).is_none());
    }

    #[test]
    fn test_none_when_single_branch_shortcircuit() {
        // Period-aligned lower bound in current period → single branch → standard path
        let m = metadata_with_pp();
        let wc = WhereClause::Field {
            path: vec!["period_start".into()],
            operator: WhereOperator::Gte,
            value: json!("2024-03-01"),
        };
        // today = Mar 20, lower bound = Mar 1 (aligned, same period) → only B3
        assert!(should_use_partial_period(&m, Some(&wc), date(2024, 3, 20)).is_none());
    }

    #[test]
    fn test_triggers_with_aligned_but_multiple_branches() {
        // Aligned lower bound but in a previous period → B2 + B3
        let m = metadata_with_pp();
        let wc = WhereClause::Field {
            path: vec!["period_start".into()],
            operator: WhereOperator::Gte,
            value: json!("2024-02-01"),
        };
        // today = Mar 20, lower bound = Feb 1 (aligned, but different period) → B2 + B3
        let result = should_use_partial_period(&m, Some(&wc), date(2024, 3, 20));
        assert!(result.is_some());
    }
}
