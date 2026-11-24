//! An aggregate `orderBy` key's direction naming where its NULLs sort (`DESC_NULLS_LAST`, …),
//! from the request to the rendered `ORDER BY`.

use super::{
    AggregationSqlGenerator, DatabaseType, create_aggregation_test_metadata, create_test_plan,
};
use crate::{
    compiler::aggregation::{OrderByClause, OrderDirection},
    runtime::AggregateQueryParser,
    types::NullsOrder,
};

fn parsed(order: &serde_json::Value) -> crate::error::Result<Vec<OrderByClause>> {
    let query = serde_json::json!({ "aggregates": [{ "revenue_sum": true }], "orderBy": order });
    AggregateQueryParser::parse(
        &query,
        &create_aggregation_test_metadata(),
        &std::collections::HashMap::new(),
    )
    .map(|request| request.order_by)
}

#[test]
fn a_key_carries_the_placement_its_direction_names_and_a_bare_direction_none() {
    let read: Vec<_> = parsed(&serde_json::json!({
        "revenue_sum": "DESC_NULLS_LAST",
        "category": "ASC_NULLS_FIRST",
        "count": "DESC",
    }))
    .unwrap()
    .iter()
    .map(|c| (c.field.clone(), c.direction, c.nulls))
    .collect();
    assert_eq!(
        read,
        [
            ("revenue_sum".to_string(), OrderDirection::Desc, Some(NullsOrder::Last)),
            ("category".to_string(), OrderDirection::Asc, Some(NullsOrder::First)),
            ("count".to_string(), OrderDirection::Desc, None),
        ]
    );
}

#[test]
fn any_other_direction_is_refused_listing_the_six() {
    let message = parsed(&serde_json::json!({ "revenue_sum": "DESC_NULLS" }))
        .unwrap_err()
        .to_string();
    assert!(
        message.contains("orderBy `revenue_sum` takes one of `ASC`, `DESC`, `ASC_NULLS_FIRST`"),
        "{message}"
    );
}

/// The rendered `ORDER BY` names a placement that is not the direction's default, and only then.
#[test]
fn the_order_by_names_the_placement() {
    let mut plan = create_test_plan();
    let mut last = OrderByClause::new("revenue_sum".to_string(), OrderDirection::Desc);
    last.nulls = Some(NullsOrder::Last);
    let mut default = OrderByClause::new("category".to_string(), OrderDirection::Asc);
    default.nulls = Some(NullsOrder::Last);
    plan.request.order_by = vec![last, default];
    let sql = AggregationSqlGenerator::new(DatabaseType::PostgreSQL)
        .generate_parameterized(&plan)
        .unwrap()
        .sql;
    assert!(
        sql.contains(r#"ORDER BY "revenue_sum" DESC NULLS LAST, "category" ASC"#),
        "{sql}"
    );
}
