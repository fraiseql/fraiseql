//! An aggregate's `orderBy` is a list of `{field, direction}` items, applied in order, as a
//! list query's and a window's are. The key-to-direction object it used to take is refused,
//! naming its key.

use super::create_aggregation_test_metadata;
use crate::{
    compiler::aggregation::{OrderByClause, OrderDirection},
    runtime::AggregateQueryParser,
    types::NullsOrder,
};

fn parsed(order: &serde_json::Value) -> crate::error::Result<Vec<OrderByClause>> {
    let query = serde_json::json!({
        "groupBy": { "category": true },
        "aggregates": [{ "revenue_sum": true }],
        "orderBy": order,
    });
    AggregateQueryParser::parse(
        &query,
        &create_aggregation_test_metadata(),
        &std::collections::HashMap::new(),
    )
    .map(|request| request.order_by)
}

fn read(clauses: &[OrderByClause]) -> Vec<(String, OrderDirection, Option<NullsOrder>)> {
    clauses.iter().map(|c| (c.field.clone(), c.direction, c.nulls)).collect()
}

#[test]
fn the_items_are_applied_in_the_order_written() {
    let clauses = parsed(&serde_json::json!([
        { "field": "revenue_sum", "direction": "DESC_NULLS_LAST" },
        { "field": "category" },
    ]))
    .unwrap();
    assert_eq!(
        read(&clauses),
        [
            ("revenue_sum".to_string(), OrderDirection::Desc, Some(NullsOrder::Last)),
            ("category".to_string(), OrderDirection::Asc, None),
        ]
    );
}

#[test]
fn one_item_written_without_the_list_is_a_one_item_list() {
    let clauses =
        parsed(&serde_json::json!({ "field": "revenue_sum", "direction": "DESC" })).unwrap();
    assert_eq!(read(&clauses), [("revenue_sum".to_string(), OrderDirection::Desc, None)]);
}

#[test]
fn the_key_to_direction_object_is_refused_naming_its_key() {
    let message = parsed(&serde_json::json!({ "revenue_sum": "DESC" })).unwrap_err().to_string();
    assert!(
        message.contains("orderBy item takes `field` and `direction`, got `revenue_sum`"),
        "{message}"
    );
}

#[test]
fn a_direction_outside_the_six_is_refused_listing_them() {
    for direction in [serde_json::json!("desc"), serde_json::json!(1)] {
        let message =
            parsed(&serde_json::json!([{ "field": "revenue_sum", "direction": direction }]))
                .unwrap_err()
                .to_string();
        assert!(
            message.contains(&format!(
                "orderBy `revenue_sum` takes one of {}, got {direction}",
                crate::backend::types::nulls_order::sort_directions_listed()
            )),
            "{message}"
        );
    }
}

/// A document writing the key-to-direction object on the aggregate root is refused naming its
/// key, before any statement; the list naming the same order is served.
#[tokio::test]
async fn a_document_writing_the_object_is_refused_and_the_list_served() {
    use std::sync::Arc;

    use crate::{
        runtime::{Executor, executor::test_support::MockAdapter},
        schema::CompiledSchema,
    };

    let mut schema = CompiledSchema::new();
    schema.add_fact_table("tf_sales".to_string(), create_aggregation_test_metadata());
    schema.build_indexes();
    let executor = Executor::new(schema, Arc::new(MockAdapter::new(Vec::new())));
    let message = executor
        .execute(
            "{ sales_aggregate(aggregates: [{revenue_sum: true}], orderBy: {revenue_sum: DESC}) \
             { revenue_sum } }",
            None,
        )
        .await
        .expect_err("the object form is refused")
        .to_string();
    assert!(
        message.contains("orderBy item takes `field` and `direction`, got `revenue_sum`"),
        "{message}"
    );
    executor
        .execute(
            "{ sales_aggregate(aggregates: [{revenue_sum: true}], orderBy: [{field: revenue_sum, \
             direction: DESC}]) { revenue_sum } }",
            None,
        )
        .await
        .expect("the list is served");
}
