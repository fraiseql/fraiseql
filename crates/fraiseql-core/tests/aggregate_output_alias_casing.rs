#![allow(clippy::unwrap_used, clippy::panic, clippy::float_cmp)] // Reason: test code; the sums compared are small exact integers

//! Issue #1516 — an aggregate or window output column answers under exactly the key the
//! request named, camelCase included.
//!
//! Every output column was emitted as `… AS {alias}` with the alias unquoted, so
//! PostgreSQL folded `itemCategory` to `itemcategory` and the response carried a key the
//! client never asked for (and its GraphQL type does not declare). The aggregate `ORDER BY`
//! quotes the alias, so ordering by a camelCase key referenced a column that did not exist.
//!
//! One case per path that emits an output alias and can run: a plain JSONB dimension, a
//! dimension mapped to a native column, the same ordered by its alias, and the window
//! planner's select and window aliases. The partial-period UNION quotes its aliases the same
//! way, but cannot execute on PostgreSQL at all (#1519), so it has no case here. Driven
//! through the real executor against PostgreSQL.
//!
//! **Execution engine:** `PostgreSQL` · **Infrastructure:** `DATABASE_URL` ·
//! **Parallelism:** creates and drops its own `tf_issue_1516` table.

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

const TABLE: &str = "tf_issue_1516";

/// Two categories, a row every ten days over the last four months.
async fn provision(adapter: &PostgresAdapter) {
    for ddl in [
        format!("DROP TABLE IF EXISTS {TABLE}"),
        format!(
            "CREATE TABLE {TABLE} (revenue numeric NOT NULL, category_id int NOT NULL, \
             period date NOT NULL, data jsonb NOT NULL)"
        ),
        format!(
            "INSERT INTO {TABLE} (revenue, category_id, period, data) \
             SELECT 1, c, current_date - d, jsonb_build_object('item_category', c::text) \
             FROM generate_series(1, 2) AS c, generate_series(0, 120, 10) AS d"
        ),
    ] {
        adapter.execute_raw_query(&ddl).await.unwrap();
    }
}

fn metadata(mapping: &[(&str, &str)]) -> FactTableMetadata {
    FactTableMetadata {
        table_name:               TABLE.to_string(),
        type_name:                None,
        measures:                 vec![MeasureColumn {
            name:     "revenue".to_string(),
            sql_type: SqlType::Decimal,
            nullable: false,
        }],
        dimensions:               DimensionColumn {
            name:  "data".to_string(),
            paths: vec![],
        },
        denormalized_filters:     vec![
            FilterColumn {
                name:      "category_id".to_string(),
                sql_type:  SqlType::Int,
                indexed:   true,
                hierarchy: None,
            },
            FilterColumn {
                name:      "period".to_string(),
                sql_type:  SqlType::Date,
                indexed:   true,
                hierarchy: None,
            },
        ],
        calendar_dimensions:      vec![],
        partial_period:           None,
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

async fn aggregate(executor: &Executor, metadata: &FactTableMetadata, query: Value) -> Vec<Value> {
    let response = executor
        .execute_aggregate_query(&query, "sales_aggregate", metadata)
        .await
        .unwrap_or_else(|e| panic!("aggregate {query} failed: {e}"));
    response["data"]["sales_aggregate"].as_array().cloned().unwrap_or_default()
}

/// The keys of every row, which must all be exactly `expected`.
fn assert_keys(rows: &[Value], expected: &[&str], case: &str) {
    assert!(!rows.is_empty(), "{case}: no rows");
    for row in rows {
        let mut keys: Vec<&str> = row.as_object().unwrap().keys().map(String::as_str).collect();
        keys.sort_unstable();
        let mut want = expected.to_vec();
        want.sort_unstable();
        assert_eq!(keys, want, "{case}: the row answers under the requested keys: {row}");
    }
}

#[tokio::test]
async fn a_camel_case_jsonb_dimension_answers_under_its_requested_key() {
    let executor = executor().await;
    let rows = aggregate(
        &executor,
        &metadata(&[]),
        json!({ "table": TABLE, "groupBy": { "itemCategory": true },
                "aggregates": [{ "revenue_sum": {} }] }),
    )
    .await;
    assert_keys(&rows, &["itemCategory", "revenue_sum"], "jsonb dimension");
}

#[tokio::test]
async fn a_camel_case_mapped_dimension_answers_under_its_requested_key() {
    let executor = executor().await;
    let rows = aggregate(
        &executor,
        &metadata(&[("itemCategory", "category_id")]),
        json!({ "table": TABLE, "groupBy": { "itemCategory": true },
                "aggregates": [{ "revenue_sum": {} }] }),
    )
    .await;
    assert_keys(&rows, &["itemCategory", "revenue_sum"], "mapped dimension");
}

/// The aggregate `ORDER BY` already quoted the alias, so ordering by a camelCase key
/// referenced a column the unquoted `SELECT` had folded away: a SQL error.
#[tokio::test]
async fn a_camel_case_dimension_can_be_ordered_by_its_alias() {
    let executor = executor().await;
    for direction in ["ASC", "DESC"] {
        let rows = aggregate(
            &executor,
            &metadata(&[]),
            json!({ "table": TABLE, "groupBy": { "itemCategory": true },
                    "aggregates": [{ "revenue_sum": {} }],
                    "orderBy": { "itemCategory": direction } }),
        )
        .await;
        assert_keys(&rows, &["itemCategory", "revenue_sum"], "ordered by alias");
        let order: Vec<&str> = rows.iter().map(|r| r["itemCategory"].as_str().unwrap()).collect();
        let expected = if direction == "ASC" {
            ["1", "2"]
        } else {
            ["2", "1"]
        };
        assert_eq!(order, expected, "ordered {direction} by the camelCase alias");
    }
}

/// The window planner takes both aliases from the request: the select column's and the
/// window function's. The final `ORDER BY` may name either.
#[tokio::test]
async fn window_aliases_answer_under_their_requested_keys() {
    let executor = executor().await;
    let response = executor
        .execute_window_query(
            &json!({
                "table": TABLE,
                "select": [
                    { "type": "dimension", "path": "item_category", "alias": "itemCategory" },
                    { "type": "measure", "name": "revenue", "alias": "dailyRevenue" }
                ],
                "windows": [{
                    "function": { "type": "row_number" },
                    "alias": "rowRank",
                    "partitionBy": [{ "type": "dimension", "path": "item_category" }],
                    "orderBy": [{ "field": "period", "direction": "ASC" }]
                }],
                "orderBy": [{ "field": "rowRank", "direction": "DESC" }]
            }),
            "sales_window",
            &metadata(&[]),
        )
        .await
        .unwrap_or_else(|e| panic!("window query failed: {e}"));
    let rows = response["data"]["sales_window"].as_array().cloned().unwrap_or_default();
    assert_keys(&rows, &["itemCategory", "dailyRevenue", "rowRank"], "window");
    let ranks: Vec<i64> = rows.iter().map(|r| r["rowRank"].as_i64().unwrap()).collect();
    let mut sorted = ranks.clone();
    sorted.sort_unstable_by(|a, b| b.cmp(a));
    assert_eq!(ranks, sorted, "ordered DESC by the camelCase window alias");
}
