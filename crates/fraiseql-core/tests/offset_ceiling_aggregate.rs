#![allow(clippy::unwrap_used, clippy::panic, clippy::print_stderr)] // Reason: test code — panics and skip diagnostics are acceptable

//! #1306: an aggregate's and a window query's `offset` are held to `[validation] max_offset`
//! as a list's is, before any statement.
//!
//! `tf_offset_boom` raises on every row, so a refused request is answered by the ceiling;
//! had a statement reached the table, PostgreSQL's exception would be the answer instead.
//! `tf_offset_sale` serves the requests within it.
//!
//! **Execution engine:** `PostgreSQL` · **Infrastructure:** `DATABASE_URL` ·
//! **Parallelism:** creates and drops its own `tf_offset_*` tables.

use std::{collections::HashMap, sync::Arc};

use fraiseql_core::{
    compiler::fact_table::{
        DimensionColumn, DimensionPath, FactTableMetadata, MeasureColumn, SqlType,
    },
    db::{DatabaseAdapter, postgres::PostgresAdapter},
    runtime::{Executor, RuntimeConfig},
    schema::CompiledSchema,
};
use serde_json::{Value, json};

const SALE: &str = "tf_offset_sale";
const BOOM: &str = "tf_offset_boom";

fn fact_table(table: &str) -> FactTableMetadata {
    FactTableMetadata {
        table_name:               table.to_string(),
        type_name:                None,
        measures:                 vec![MeasureColumn {
            name:     "qty".to_string(),
            sql_type: SqlType::BigInt,
            nullable: false,
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
        partial_period:           None,
        native_measures:          HashMap::new(),
        native_dimension_mapping: HashMap::new(),
    }
}

async fn executor() -> Option<Executor> {
    let url = fraiseql_test_support::try_database_url()?;
    let adapter = PostgresAdapter::new(&url).await.unwrap();
    for ddl in [
        format!("DROP VIEW IF EXISTS {BOOM}"),
        format!("DROP TABLE IF EXISTS {SALE}"),
        "DROP FUNCTION IF EXISTS tf_offset_boom_row(bigint)".to_string(),
        format!("CREATE TABLE {SALE} (id bigint, qty bigint, data jsonb)"),
        format!(
            "INSERT INTO {SALE} SELECT k, k, jsonb_build_object('region', 'r' || k) FROM \
             generate_series(1, 4) k"
        ),
        "CREATE FUNCTION tf_offset_boom_row(bigint) RETURNS jsonb LANGUAGE plpgsql AS $$ BEGIN \
         RAISE EXCEPTION 'tf_offset_boom was read'; END $$"
            .to_string(),
        format!("CREATE VIEW {BOOM} AS SELECT id, qty, tf_offset_boom_row(id) AS data FROM {SALE}"),
    ] {
        adapter.execute_raw_query(&ddl).await.unwrap();
    }
    let config = RuntimeConfig {
        max_offset: Some(2),
        ..RuntimeConfig::default()
    };
    Some(Executor::with_config(CompiledSchema::new(), Arc::new(adapter), config))
}

fn regions(response: &Value, key: &str) -> Vec<Value> {
    response["data"][key]
        .as_array()
        .unwrap_or_else(|| panic!("{response}"))
        .iter()
        .map(|g| g["region"].clone())
        .collect()
}

fn assert_refused_before_any_statement(err: &str) {
    assert!(!err.contains("was read"), "refused before any statement: {err}");
    assert!(err.contains("max_offset") && err.contains("relay = true"), "{err}");
}

#[tokio::test]
async fn an_aggregate_offset_past_the_ceiling_is_refused_before_any_statement() {
    let Some(executor) = executor().await else {
        eprintln!("skipping #1306: DATABASE_URL not set");
        return;
    };
    let query = |table: &str, offset: u32| {
        json!({
            "table": table,
            "groupBy": { "region": true },
            "aggregates": [{ "count": {} }],
            "orderBy": { "region": "ASC" },
            "offset": offset
        })
    };
    let served = executor
        .execute_aggregate_query(&query(SALE, 2), "sales_aggregate", &fact_table(SALE))
        .await
        .unwrap();
    assert_eq!(regions(&served, "sales_aggregate"), vec![json!("r3"), json!("r4")]);

    let err = executor
        .execute_aggregate_query(&query(BOOM, 3), "booms_aggregate", &fact_table(BOOM))
        .await
        .expect_err("refused")
        .to_string();
    assert_refused_before_any_statement(&err);
}

#[tokio::test]
async fn a_window_offset_past_the_ceiling_is_refused_before_any_statement() {
    let Some(executor) = executor().await else {
        return;
    };
    let query = |table: &str, offset: u32| {
        json!({
            "table": table,
            "select": [{ "type": "dimension", "path": "region", "alias": "region" }],
            "windows": [{ "function": { "type": "row_number" }, "alias": "position" }],
            "orderBy": [{ "field": "region", "direction": "ASC" }],
            "offset": offset
        })
    };
    let served = executor
        .execute_window_query(&query(SALE, 2), "sales_window", &fact_table(SALE))
        .await
        .unwrap();
    assert_eq!(regions(&served, "sales_window"), vec![json!("r3"), json!("r4")]);

    let err = executor
        .execute_window_query(&query(BOOM, 3), "booms_window", &fact_table(BOOM))
        .await
        .expect_err("refused")
        .to_string();
    assert_refused_before_any_statement(&err);
}
