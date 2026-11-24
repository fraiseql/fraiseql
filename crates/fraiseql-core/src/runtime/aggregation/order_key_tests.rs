//! An aggregate's `orderBy` sorts by a group key or a scalar aggregate. A collection aggregate
//! (`json_agg`, `jsonb_agg`, `array_agg`) is a JSON document or an array, which compares as
//! no order a client asked for, as a list query does not sort by a list or a JSON field: the
//! planner refuses it, and the schema's `<Name>AggregateOrderByField` does not offer it.

use std::sync::Arc;

use super::create_aggregation_test_metadata;
use crate::{
    compiler::{
        aggregation::AggregationPlanner,
        fact_table::{FactTableMetadata, FilterColumn, SqlType},
    },
    runtime::{AggregateQueryParser, Executor, executor::test_support::MockAdapter},
    schema::CompiledSchema,
};

/// The test fact table with a text filter column, whose `array_agg` and `string_agg` exist.
fn metadata() -> FactTableMetadata {
    let mut metadata = create_aggregation_test_metadata();
    metadata.denormalized_filters.push(FilterColumn {
        name:      "region".to_string(),
        sql_type:  SqlType::Text,
        indexed:   false,
        hierarchy: None,
    });
    metadata
}

/// The planner's answer to computing `aggregate` and sorting the groups by it.
fn planned(aggregate: &str) -> crate::error::Result<()> {
    let query = serde_json::json!({
        "groupBy": { "category": true },
        "aggregates": [{ aggregate: true }],
        "orderBy": [{ "field": aggregate, "direction": "DESC" }],
    });
    let request =
        AggregateQueryParser::parse(&query, &metadata(), &std::collections::HashMap::new())?;
    AggregationPlanner::plan(request, metadata()).map(|_| ())
}

#[test]
fn a_collection_aggregate_is_refused_as_a_sort_key() {
    for aggregate in ["revenue_json_agg", "revenue_jsonb_agg", "region_array_agg"] {
        let message = planned(aggregate).expect_err(aggregate).to_string();
        assert!(
            message.contains(&format!(
                "orderBy `{aggregate}` is a JSON document or an array, which does not sort"
            )),
            "{aggregate}: {message}"
        );
    }
}

#[test]
fn a_scalar_aggregate_sorts() {
    for aggregate in [
        "revenue_sum",
        "revenue_count_distinct",
        "region_string_agg",
        "count",
    ] {
        planned(aggregate).unwrap_or_else(|e| panic!("{aggregate}: {e}"));
    }
}

/// A document naming one is refused before any statement; the aggregate is still computed.
#[tokio::test]
async fn a_document_sorting_by_a_collection_aggregate_is_refused() {
    let mut schema = CompiledSchema::new();
    schema.add_fact_table("tf_sales".to_string(), metadata());
    schema.build_indexes();
    let executor = Executor::new(schema, Arc::new(MockAdapter::new(Vec::new())));
    let message = executor
        .execute(
            "{ sales_aggregate(aggregates: [{revenue_json_agg: true}], orderBy: [{field: \
             revenue_json_agg}]) { revenue_json_agg } }",
            None,
        )
        .await
        .expect_err("not a sort key")
        .to_string();
    assert!(
        message.contains("orderBy `revenue_json_agg` is a JSON document or an array"),
        "{message}"
    );
    executor
        .execute(
            "{ sales_aggregate(aggregates: [{revenue_json_agg: true}, {revenue_sum: true}], \
             orderBy: [{field: revenue_sum}]) { revenue_json_agg } }",
            None,
        )
        .await
        .expect("a collection aggregate is still computed");
}
