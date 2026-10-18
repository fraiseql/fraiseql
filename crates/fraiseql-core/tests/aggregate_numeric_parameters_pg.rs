#![allow(clippy::unwrap_used, clippy::panic, clippy::print_stderr)] // Reason: test code — panics and skip diagnostics are acceptable

//! #1620: an aggregate's parameters are written for the type the server resolves them to.
//!
//! `HAVING SUM(qty) > $1` over a `bigint` measure makes `$1` a `numeric`, and the number was
//! sent as text bytes under the binary format: PostgreSQL refused every such request with
//! `insufficient data left in message`.
//!
//! **Execution engine:** `PostgreSQL` · **Infrastructure:** `DATABASE_URL` ·
//! **Parallelism:** creates and drops its own `tf_numeric_param_sale` table.

use std::{collections::HashMap, sync::Arc};

use fraiseql_core::{
    compiler::fact_table::{
        DimensionColumn, DimensionPath, FactTableMetadata, MeasureColumn, SqlType,
    },
    db::{DatabaseAdapter, postgres::PostgresAdapter},
    runtime::Executor,
    schema::CompiledSchema,
};
use serde_json::json;

const TABLE: &str = "tf_numeric_param_sale";

fn fact_table() -> FactTableMetadata {
    FactTableMetadata {
        table_name:               TABLE.to_string(),
        type_name:                None,
        measures:                 vec![MeasureColumn {
            name:       "qty".to_string(),
            sql_type:   SqlType::BigInt,
            nullable:   false,
            additivity: fraiseql_core::compiler::fact_table::Additivity::Additive,
        }],
        dimensions:               DimensionColumn {
            name:  "data".to_string(),
            paths: vec![DimensionPath {
                name:      "region".to_string(),
                json_path: "data->>'region'".to_string(),
                data_type: "string".to_string(),
            }],
        },
        denormalized_filters:     vec![],
        calendar_dimensions:      vec![],
        native_measures:          HashMap::new(),
        native_dimension_mapping: HashMap::new(),
    }
}

#[tokio::test]
async fn a_having_bound_against_a_sum_of_integers_keeps_the_groups_over_it() {
    let Some(url) = fraiseql_test_support::try_database_url() else {
        eprintln!("skipping #1620: DATABASE_URL not set");
        return;
    };
    let adapter = PostgresAdapter::new(&url).await.unwrap();
    for ddl in [
        format!("DROP TABLE IF EXISTS {TABLE}"),
        format!("CREATE TABLE {TABLE} (id bigint, qty bigint, data jsonb)"),
        format!(
            "INSERT INTO {TABLE} VALUES (1, 5, '{{\"region\": \"r1\"}}'), \
             (2, 1, '{{\"region\": \"r2\"}}'), (3, 2, '{{\"region\": \"r3\"}}')"
        ),
    ] {
        adapter.execute_raw_query(&ddl).await.unwrap();
    }
    let executor = Executor::new(CompiledSchema::new(), Arc::new(adapter));

    let response = executor
        .execute_aggregate_query(
            &json!({
                "table": TABLE,
                "groupBy": { "region": true },
                "aggregates": [{ "qty_sum": {} }],
                "having": { "qty_sum_gt": 1 },
                "orderBy": { "region": "ASC" }
            }),
            "numeric_param_sales_aggregate",
            &fact_table(),
        )
        .await
        .unwrap();

    let regions: Vec<&str> = response["data"]["numeric_param_sales_aggregate"]
        .as_array()
        .unwrap_or_else(|| panic!("{response}"))
        .iter()
        .map(|g| g["region"].as_str().unwrap())
        .collect();
    assert_eq!(regions, ["r1", "r3"], "{response}");
}
