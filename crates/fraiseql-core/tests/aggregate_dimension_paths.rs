#![allow(clippy::unwrap_used, clippy::panic, clippy::float_cmp)] // Reason: test code; the sums compared are small exact integers

//! Issue #1517 §3 — a declared dimension path is read at its own JSONB location by
//! `groupBy` and by `where`, whatever its depth.
//!
//! A fact table declares `machine_model_category` at `data->'machine'->'model'->>'category'`
//! (the shape `introspect facts` detects: dots become underscores in the name). The runtime
//! ignored the declared location and read `data->>'machine_model_category'`, a key no row
//! has: `groupBy` put every row in one `null` group and `where …_eq` matched nothing, with no
//! error. Driven through the real executor against PostgreSQL.
//!
//! **Execution engine:** `PostgreSQL` · **Infrastructure:** `DATABASE_URL` ·
//! **Parallelism:** creates and drops its own `tf_issue_1517` table.

mod common;

use std::{collections::HashMap, sync::Arc};

use fraiseql_core::{
    compiler::fact_table::{
        DimensionColumn, DimensionPath, FactTableMetadata, MeasureColumn, SqlType,
    },
    db::{DatabaseAdapter, postgres::PostgresAdapter},
    runtime::Executor,
    schema::CompiledSchema,
};
use serde_json::{Value, json};

const TABLE: &str = "tf_issue_1517";

/// Category A earns 1 + 2, category B earns 10. A top-level `machine_model_category` key that
/// says the opposite is planted on every row, so reading the flattened name instead of the
/// declared path is visible in the totals rather than only as nulls.
async fn provision(adapter: &PostgresAdapter) {
    for ddl in [
        format!("DROP TABLE IF EXISTS {TABLE}"),
        format!("CREATE TABLE {TABLE} (revenue numeric NOT NULL, data jsonb NOT NULL)"),
        format!(
            "INSERT INTO {TABLE} (revenue, data) VALUES \
             (1,  '{{\"machine\": {{\"model\": {{\"category\": \"A\"}}}}, \"machine_model_category\": \"B\"}}'), \
             (2,  '{{\"machine\": {{\"model\": {{\"category\": \"A\"}}}}, \"machine_model_category\": \"B\"}}'), \
             (10, '{{\"machine\": {{\"model\": {{\"category\": \"B\"}}}}, \"machine_model_category\": \"A\"}}')"
        ),
    ] {
        adapter.execute_raw_query(&ddl).await.unwrap();
    }
}

fn metadata() -> FactTableMetadata {
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
            paths: vec![DimensionPath {
                name:      "machine_model_category".to_string(),
                json_path: "data->'machine'->'model'->>'category'".to_string(),
                data_type: "string".to_string(),
            }],
        },
        denormalized_filters:     vec![],
        calendar_dimensions:      vec![],
        native_measures:          HashMap::new(),
        native_dimension_mapping: HashMap::new(),
    }
}

async fn executor() -> Executor {
    let container = common::testcontainer::get_test_container().await;
    let adapter = Arc::new(PostgresAdapter::new(&container.connection_string()).await.unwrap());
    provision(&adapter).await;
    Executor::new(CompiledSchema::new(), adapter)
}

async fn aggregate(executor: &Executor, query: Value) -> Vec<Value> {
    let response = executor
        .execute_aggregate_query(&query, "sales_aggregate", &metadata())
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

#[tokio::test]
async fn a_deep_declared_path_groups_at_its_location() {
    let executor = executor().await;
    let rows = aggregate(
        &executor,
        json!({ "table": TABLE, "groupBy": { "machine_model_category": true },
                "aggregates": [{ "revenue_sum": {} }],
                "orderBy": { "machine_model_category": "ASC" } }),
    )
    .await;
    let groups: Vec<(Value, f64)> =
        rows.iter().map(|r| (r["machine_model_category"].clone(), total(r))).collect();
    assert_eq!(groups, vec![(json!("A"), 3.0), (json!("B"), 10.0)]);
}

#[tokio::test]
async fn a_deep_declared_path_filters_at_its_location() {
    let executor = executor().await;
    let rows = aggregate(
        &executor,
        json!({ "table": TABLE, "where": { "machine_model_category_eq": "A" },
                "aggregates": [{ "revenue_sum": {} }] }),
    )
    .await;
    assert_eq!(rows.first().map_or(0.0, total), 3.0, "where reads the declared path");
}

/// The window parser splits `where` keys with the aggregate parser's helper; it resolves
/// them the same way.
#[tokio::test]
async fn a_window_filters_a_deep_declared_path_at_its_location() {
    let executor = executor().await;
    let response = executor
        .execute_window_query(
            &json!({
                "table": TABLE,
                "select": [{ "type": "measure", "name": "revenue", "alias": "revenue" }],
                "windows": [{ "function": { "type": "row_number" }, "alias": "n",
                              "orderBy": [{ "field": "revenue", "direction": "ASC" }] }],
                "where": { "machine_model_category_eq": "A" }
            }),
            "sales_window",
            &metadata(),
        )
        .await
        .unwrap_or_else(|e| panic!("window query failed: {e}"));
    let mut revenues: Vec<i64> = response["data"]["sales_window"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["revenue"].as_i64().unwrap_or_else(|| panic!("revenue in {r}")))
        .collect();
    revenues.sort_unstable();
    assert_eq!(revenues, vec![1, 2], "the window keeps the category-A rows");
}
