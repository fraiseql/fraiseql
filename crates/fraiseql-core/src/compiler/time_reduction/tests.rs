use serde_json::json;

use super::*;

fn on(column: &str, operator: WhereOperator) -> WhereClause {
    WhereClause::NativeField {
        column: column.to_string(),
        pg_cast: "date".to_string(),
        operator,
        value: json!("2026-01-01"),
    }
}

/// Bounds are read from the top-level conjunction, however it nests, as a native column
/// or a field path; every other conjunct is kept apart.
#[test]
fn bounds_are_read_from_the_top_level_conjunction() {
    let clause = WhereClause::And(vec![
        on("day", WhereOperator::Gte),
        WhereClause::And(vec![on("account_id", WhereOperator::Eq)]),
        WhereClause::Field {
            path:     vec!["day".to_string()],
            operator: WhereOperator::Lt,
            value:    json!("2026-04-01"),
        },
    ]);
    let split = time_bounds(&clause, "day").expect("comparisons joined by and");
    assert_eq!(split.bounds.len(), 2);
    assert_eq!(split.rest, vec![&on("account_id", WhereOperator::Eq)]);
}

/// `day` under `or` or `not`, compared other than by a bound, or against null, cannot
/// bound a carried value.
#[test]
fn the_time_column_anywhere_else_is_not_a_bound() {
    for clause in [
        WhereClause::Or(vec![
            on("day", WhereOperator::Gte),
            on("account_id", WhereOperator::Eq),
        ]),
        WhereClause::And(vec![WhereClause::Not(Box::new(on("day", WhereOperator::Lt)))]),
        on("day", WhereOperator::Neq),
        WhereClause::NativeField {
            column:   "day".to_string(),
            pg_cast:  "date".to_string(),
            operator: WhereOperator::Lte,
            value:    serde_json::Value::Null,
        },
    ] {
        assert!(time_bounds(&clause, "day").is_none(), "{clause:?}");
    }
    // Under `or`, a clause that does not read `day` is any other conjunct.
    let other = WhereClause::Or(vec![
        on("account_id", WhereOperator::Eq),
        on("branch", WhereOperator::Eq),
    ]);
    assert_eq!(time_bounds(&other, "day").expect("no time column").rest.len(), 1);
}
