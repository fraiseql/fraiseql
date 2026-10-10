#![allow(clippy::unwrap_used, clippy::panic)] // Reason: test code, panics are acceptable
#![allow(clippy::float_cmp)] // Reason: every total is a sum of small powers of two, exact in f64
#![allow(missing_docs)]

//! Issue #1460 — a fact-table aggregate applies hierarchy and underscore-spelled filters.
//!
//! The aggregate `where` parser split `field_operator` keys at the last underscore, so
//! `path_descendant_of` or `customer_id_is_not_null` could not be spelled at all; and the
//! aggregate generator implemented no ltree operator, so `descendantOf` was refused. Driven
//! through the real executor against PostgreSQL: a fact table with an `ltree` filter column
//! and an ltree path inside its JSONB dimensions.

mod common;

use std::{collections::HashMap, sync::Arc};

use fraiseql_core::{
    compiler::fact_table::{
        DimensionColumn, DimensionPath, FactTableMetadata, FilterColumn, MeasureColumn, SqlType,
    },
    db::{DatabaseAdapter, postgres::PostgresAdapter},
    runtime::Executor,
    schema::CompiledSchema,
};
use serde_json::{Value, json};

const TABLE: &str = "tf_issue_1460";

async fn provision(adapter: &PostgresAdapter) {
    adapter.execute_raw_query("CREATE EXTENSION IF NOT EXISTS ltree").await.unwrap();
    adapter
        .execute_raw_query(&format!("DROP TABLE IF EXISTS {TABLE}"))
        .await
        .unwrap();
    adapter
        .execute_raw_query(&format!(
            "CREATE TABLE {TABLE} (revenue numeric NOT NULL, path ltree NOT NULL, \
             customer_id text, data jsonb NOT NULL)"
        ))
        .await
        .unwrap();
    adapter
        .execute_raw_query(&format!(
            "INSERT INTO {TABLE} VALUES \
             (1, 'a',     'c1', '{{\"region\": \"eu\"}}'), \
             (2, 'a.b',   'c2', '{{\"region\": \"eu.fr\"}}'), \
             (4, 'a.b.c', NULL, '{{\"region\": \"eu.fr.paris\"}}'), \
             (8, 'a.x',   'c3', '{{\"region\": \"us\"}}'), \
             (16, 'z',    NULL, '{{\"region\": \"us.ny\"}}')"
        ))
        .await
        .unwrap();
}

fn metadata() -> FactTableMetadata {
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
            paths: vec![DimensionPath {
                name:      "region".to_string(),
                json_path: "data->>'region'".to_string(),
                data_type: "text".to_string(),
            }],
        },
        denormalized_filters:     vec![
            FilterColumn {
                name:      "path".to_string(),
                sql_type:  SqlType::Other("ltree".to_string()),
                indexed:   true,
                hierarchy: None,
            },
            FilterColumn {
                name:      "customer_id".to_string(),
                sql_type:  SqlType::Text,
                indexed:   true,
                hierarchy: None,
            },
        ],
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

/// Total revenue over the rows the `where` filter selects. Revenues are distinct powers of
/// two, so the total names exactly which rows were aggregated.
async fn revenue(executor: &Executor, filter: Value) -> f64 {
    let query = json!({
        "table": TABLE,
        "where": filter,
        "aggregates": [{ "revenue_sum": {} }],
    });
    let response = executor
        .execute_aggregate_query(&query, "sales_aggregate", &metadata())
        .await
        .unwrap_or_else(|e| panic!("aggregate with where {filter} failed: {e}"));
    let row = &response["data"]["sales_aggregate"][0];
    row["revenue_sum"]
        .as_f64()
        .or_else(|| row["revenue_sum"].as_str().and_then(|s| s.parse().ok()))
        .unwrap_or_else(|| panic!("no revenue_sum in {response}"))
}

#[tokio::test]
async fn hierarchy_and_underscore_spelled_operators_select_their_rows() {
    let executor = executor().await;

    // a.b, a.b.c
    assert_eq!(revenue(&executor, json!({ "path_descendant_of": "a.b" })).await, 6.0);
    assert_eq!(revenue(&executor, json!({ "path_descendantOf": "a.b" })).await, 6.0);
    assert_eq!(revenue(&executor, json!({ "path_isdescendant": "a.b" })).await, 6.0);
    // a, a.b, a.b.c
    assert_eq!(revenue(&executor, json!({ "path_ancestor_of": "a.b.c" })).await, 7.0);
    // a.b, a.x
    assert_eq!(revenue(&executor, json!({ "path_matches_lquery": "a.*{1}" })).await, 10.0);
    assert_eq!(
        revenue(&executor, json!({ "path_matches_any_lquery": ["a.b.*{1}", "z"] })).await,
        20.0
    );
    assert_eq!(revenue(&executor, json!({ "path_matches_ltxtquery": "x" })).await, 8.0);
    // nlevel(path) = 2: a.b, a.x — `path_depth_eq` splits as `path` + `depth_eq` because
    // `path` is a declared filter column and `path_depth` is not.
    assert_eq!(revenue(&executor, json!({ "path_depth_eq": 2 })).await, 10.0);
    assert_eq!(revenue(&executor, json!({ "path_depth_gte": 2 })).await, 14.0);
    // customer_id IS NOT NULL: a, a.b, a.x
    assert_eq!(revenue(&executor, json!({ "customer_id_is_not_null": true })).await, 11.0);
    // A JSONB dimension holding an ltree path: eu.fr, eu.fr.paris
    assert_eq!(revenue(&executor, json!({ "region_descendant_of": "eu.fr" })).await, 6.0);
}
