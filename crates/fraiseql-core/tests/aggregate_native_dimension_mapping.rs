#![allow(clippy::unwrap_used, clippy::panic, clippy::float_cmp)] // Reason: test code; the sums compared are small exact integers

//! Issue #1231 — a dimension mapped to a native column (`native_dimension_mapping`) is read
//! from that column by `where` as well as by `groupBy`, whichever casing the mapping key
//! is declared in.
//!
//! The mapping was consulted by `groupBy` only: a fact table that declares
//! `category → category_id` grouped on the column and filtered on `data->>'category'`, the
//! expensive half. And it was looked up by the raw request key, so a mapping declared in
//! the other casing (`item_category` for a request's `itemCategory`) never fired, silently.
//!
//! The fixture's column and its JSONB copy **disagree on purpose**, so which one a request
//! read is visible in the rows it returns. Driven through the real executor against
//! PostgreSQL.
//!
//! **Execution engine:** `PostgreSQL` · **Infrastructure:** `DATABASE_URL` ·
//! **Parallelism:** creates and drops its own `tf_issue_1231` table.

mod common;

use std::{collections::HashMap, sync::Arc};

use fraiseql_core::{
    compiler::fact_table::{
        DimensionColumn, FactTableMetadata, FilterColumn, MeasureColumn, SqlType,
    },
    db::{DatabaseAdapter, postgres::PostgresAdapter},
    runtime::Executor,
    schema::CompiledSchema,
};
use serde_json::{Value, json};

const TABLE: &str = "tf_issue_1231";

/// Two rows whose `category_id` column says the opposite of their JSONB `category`.
async fn provision(adapter: &PostgresAdapter) {
    adapter
        .execute_raw_query(&format!("DROP TABLE IF EXISTS {TABLE}"))
        .await
        .unwrap();
    adapter
        .execute_raw_query(&format!(
            "CREATE TABLE {TABLE} (revenue numeric NOT NULL, category_id int NOT NULL, \
             data jsonb NOT NULL)"
        ))
        .await
        .unwrap();
    adapter
        .execute_raw_query(&format!(
            "INSERT INTO {TABLE} (revenue, category_id, data) VALUES \
             (1, 2, '{{\"category\": \"1\", \"item_category\": \"1\"}}'), \
             (10, 1, '{{\"category\": \"2\", \"item_category\": \"2\"}}')"
        ))
        .await
        .unwrap();
}

/// The fact table, with `mapping` as its `native_dimension_mapping`.
fn metadata(mapping: &[(&str, &str)]) -> FactTableMetadata {
    FactTableMetadata {
        table_name:               TABLE.to_string(),
        type_name:                None,
        measures:                 vec![MeasureColumn {
            name:       "revenue".to_string(),
            sql_type:   SqlType::Decimal,
            nullable:   false,
            additivity: fraiseql_core::compiler::fact_table::Additivity::Additive,
        }],
        dimensions:               DimensionColumn {
            name:  "data".to_string(),
            paths: vec![],
        },
        denormalized_filters:     vec![FilterColumn {
            name:      "category_id".to_string(),
            sql_type:  SqlType::Int,
            indexed:   true,
            hierarchy: None,
        }],
        calendar_dimensions:      vec![],
        native_measures:          HashMap::new(),
        native_dimension_mapping: mapping
            .iter()
            .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
            .collect(),
    }
}

async fn executor() -> Executor {
    let container = common::testcontainer::get_test_container().await;
    let adapter = Arc::new(PostgresAdapter::new(&container.connection_string()).await.unwrap());
    provision(&adapter).await;
    Executor::new(CompiledSchema::new(), adapter)
}

async fn run(executor: &Executor, metadata: &FactTableMetadata, query: Value) -> Vec<Value> {
    let response = executor
        .execute_aggregate_query(&query, "sales_aggregate", metadata)
        .await
        .unwrap_or_else(|e| panic!("aggregate {query} failed: {e}"));
    response["data"]["sales_aggregate"].as_array().cloned().unwrap_or_default()
}

fn total(row: &Value) -> f64 {
    row["revenue_sum"]
        .as_f64()
        .or_else(|| row["revenue_sum"].as_str().and_then(|s| s.parse().ok()))
        .unwrap_or_else(|| panic!("no revenue_sum in {row}"))
}

/// Revenue of the rows a `where` keeps.
async fn revenue_where(executor: &Executor, metadata: &FactTableMetadata, filter: Value) -> f64 {
    let rows = run(
        executor,
        metadata,
        json!({ "table": TABLE, "where": filter, "aggregates": [{ "revenue_sum": {} }] }),
    )
    .await;
    rows.first().map_or(0.0, total)
}

/// `{group value → revenue}` for a `groupBy` on `key`.
async fn grouped(
    executor: &Executor,
    metadata: &FactTableMetadata,
    key: &str,
) -> Vec<(String, f64)> {
    let rows = run(
        executor,
        metadata,
        json!({ "table": TABLE, "groupBy": { key: true }, "aggregates": [{ "revenue_sum": {} }] }),
    )
    .await;
    let mut groups: Vec<(String, f64)> = rows
        .iter()
        .map(|row| {
            let value = row.get(key).unwrap_or_else(|| panic!("no `{key}` in {row}"));
            (value.as_str().map_or_else(|| value.to_string(), str::to_string), total(row))
        })
        .collect();
    groups.sort_by(|a, b| a.0.cmp(&b.0));
    groups
}

/// A mapped `groupBy` reads the column, and answers under the key the request asked for:
/// `category`, not the column's name `category_id`, which no selection of the dimension
/// would find.
#[tokio::test]
async fn a_mapped_dimension_groups_under_the_requested_key() {
    let executor = executor().await;
    let metadata = metadata(&[("category", "category_id")]);

    assert_eq!(
        grouped(&executor, &metadata, "category").await,
        vec![("1".to_string(), 10.0), ("2".to_string(), 1.0)],
        "groupBy reads category_id and answers as `category`"
    );
}

/// The column says category 2 is the row with revenue 1; the JSONB says it is the row with
/// revenue 10. A mapped `where` must read the column, as `groupBy` does.
#[tokio::test]
async fn a_mapped_dimension_filters_on_its_column() {
    let executor = executor().await;
    let metadata = metadata(&[("category", "category_id")]);

    assert_eq!(
        revenue_where(&executor, &metadata, json!({ "category_eq": 2 })).await,
        1.0,
        "where must read category_id, not data->>'category'"
    );
}

/// The mapped column is an `int`; a filter value given as a string is cast to the column's
/// type, as a denormalized filter's is, rather than compared as text.
#[tokio::test]
async fn a_mapped_filter_casts_its_value_to_the_column_type() {
    let executor = executor().await;
    let metadata = metadata(&[("category", "category_id")]);

    assert_eq!(
        revenue_where(&executor, &metadata, json!({ "category_eq": "2" })).await,
        1.0,
        "a string literal compares with the int column as an int"
    );
    assert_eq!(
        revenue_where(&executor, &metadata, json!({ "category_in": ["2"] })).await,
        1.0,
        "and so does a list"
    );
}

/// A mapping fires whichever casing its key is declared in: the request may say
/// `itemCategory` or `item_category`, and the mapping may be declared either way.
#[tokio::test]
async fn a_mapping_fires_in_either_casing() {
    let executor = executor().await;

    for declared in ["itemCategory", "item_category"] {
        let metadata = metadata(&[(declared, "category_id")]);
        for requested in ["itemCategory", "item_category"] {
            assert_eq!(
                grouped(&executor, &metadata, requested).await,
                vec![("1".to_string(), 10.0), ("2".to_string(), 1.0)],
                "mapping declared `{declared}`, groupBy `{requested}`: grouped on the column"
            );
            assert_eq!(
                revenue_where(&executor, &metadata, json!({ format!("{requested}_eq"): 2 })).await,
                1.0,
                "mapping declared `{declared}`, where `{requested}_eq`: filtered on the column"
            );
        }
    }
}

/// A denormalized `int` filter column, mapped or not, compares with a number. The value is
/// bound as text, and a bare `$1::int4` decoded those bytes as a binary integer
/// ("insufficient data left in message"), so no `int` filter column could be filtered.
#[tokio::test]
async fn an_int_filter_column_filters_by_a_number() {
    let executor = executor().await;
    let metadata = metadata(&[]);

    assert_eq!(
        revenue_where(&executor, &metadata, json!({ "category_id_eq": 2 })).await,
        1.0,
        "category_id = 2"
    );
    assert_eq!(
        revenue_where(&executor, &metadata, json!({ "category_id_in": [1, 2] })).await,
        11.0,
        "category_id IN (1, 2)"
    );
}

/// Revenue of the rows a window query's `where` keeps.
async fn window_revenue_where(
    executor: &Executor,
    metadata: &FactTableMetadata,
    filter: Value,
) -> f64 {
    let response = executor
        .execute_window_query(
            &json!({
                "table": TABLE,
                "select": [{ "type": "measure", "name": "revenue", "alias": "revenue" }],
                "windows": [{ "function": { "type": "row_number" }, "alias": "n",
                              "orderBy": [{ "field": "revenue", "direction": "ASC" }] }],
                "where": filter
            }),
            "sales_window",
            metadata,
        )
        .await
        .unwrap_or_else(|e| panic!("window query failed: {e}"));
    response["data"]["sales_window"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["revenue"].as_f64().unwrap_or_else(|| panic!("revenue in {r}")))
        .sum()
}

/// #1517: a window query resolves a `where` key as an aggregate does. Its parser handed
/// every key to the JSONB column, so a filter column (`category_id`, which no JSONB key
/// carries) kept nothing, and a mapped dimension read the JSONB copy, not its column.
#[tokio::test]
async fn a_window_filters_a_filter_column_and_a_mapped_dimension_on_their_column() {
    let executor = executor().await;

    assert_eq!(
        window_revenue_where(&executor, &metadata(&[]), json!({ "category_id_eq": 2 })).await,
        1.0,
        "a filter column is read from the column"
    );
    assert_eq!(
        window_revenue_where(
            &executor,
            &metadata(&[("category", "category_id")]),
            json!({ "category_eq": 2 })
        )
        .await,
        1.0,
        "a mapped dimension is read from its column, not data->>'category'"
    );
}
