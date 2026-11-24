//! A window `orderBy` item's direction naming where its NULLs sort (`DESC_NULLS_LAST`, …),
//! from the request to the SQL: inside `OVER (…)` and in the window rows' own `ORDER BY`.

use serde_json::json;

use super::{WindowPlanner, create_test_metadata};
use crate::{
    error::Result,
    runtime::{WindowQueryParser, window::WindowSqlGenerator},
    types::{NullsOrder, OrderDirection},
};

fn sql(variables: &serde_json::Value) -> Result<String> {
    let metadata = create_test_metadata();
    let plan = WindowPlanner::plan(WindowQueryParser::parse(variables, &metadata)?, &metadata)?;
    Ok(WindowSqlGenerator::new(crate::backend::DatabaseType::PostgreSQL)
        .generate(&plan)?
        .raw_sql)
}

fn ranked(order: &serde_json::Value) -> serde_json::Value {
    json!({
        "select": [{ "type": "measure", "name": "revenue", "alias": "revenue" }],
        "windows": [{ "function": { "type": "rank" }, "alias": "r", "orderBy": order }],
        "orderBy": order
    })
}

#[test]
fn an_item_carries_the_placement_its_direction_names_and_a_bare_direction_none() {
    let metadata = create_test_metadata();
    let request = WindowQueryParser::parse(
        &ranked(&json!([
            { "field": "revenue", "direction": "DESC_NULLS_LAST" },
            { "field": "quantity", "direction": "DESC" },
            { "field": "occurred_at" }
        ])),
        &metadata,
    )
    .unwrap();
    let expected = [
        (OrderDirection::Desc, Some(NullsOrder::Last)),
        (OrderDirection::Desc, None),
        (OrderDirection::Asc, None),
    ];
    let read = |items: &[crate::compiler::window_functions::WindowOrderBy]| -> Vec<_> {
        items.iter().map(|o| (o.direction, o.nulls)).collect()
    };
    assert_eq!(read(&request.windows[0].order_by), expected);
    assert_eq!(read(&request.order_by), expected);
}

#[test]
fn any_other_direction_is_refused_listing_the_six() {
    let message = sql(&ranked(&json!([{ "field": "revenue", "direction": "DESC_NULLS" }])))
        .unwrap_err()
        .to_string();
    assert!(
        message.contains("direction takes one of `ASC`, `DESC`, `ASC_NULLS_FIRST`"),
        "{message}"
    );
}

/// Both `ORDER BY`s name a placement that is not the direction's default, and only then.
#[test]
fn the_over_clause_and_the_rows_order_name_the_placement() {
    let sql = sql(&ranked(&json!([
        { "field": "revenue", "direction": "DESC_NULLS_LAST" },
        { "field": "quantity", "direction": "ASC_NULLS_LAST" }
    ])))
    .unwrap();
    assert!(
        sql.contains("RANK() OVER (ORDER BY revenue DESC NULLS LAST, quantity ASC)"),
        "{sql}"
    );
    assert!(sql.ends_with("ORDER BY revenue DESC NULLS LAST, quantity ASC"), "{sql}");
}
