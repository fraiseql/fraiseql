#![allow(clippy::unwrap_used, clippy::panic, clippy::print_stderr)] // Reason: test code — panics and skip diagnostics are acceptable

//! #1609: an aggregate or window root field reads its arguments as a regular query does,
//! written inline or passed as variables, through `Executor::execute` with a document.
//!
//! It read the request's variables alone, and refused any argument written on the root
//! (#1532), so the obvious GraphQL form `sales_aggregate(limit: 2)` was refused. Each case
//! asks for an answer that differs from the unfiltered, unordered, unlimited one.
//!
//! **Execution engine:** `PostgreSQL` · **Infrastructure:** `DATABASE_URL` ·
//! **Parallelism:** creates and drops its own `tf_inline_sale` table.

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

const TABLE: &str = "tf_inline_sale";

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

/// Regions `r1` (qty 5), `r2` (qty 5), `r3` (qty 1), `r4` (qty 2).
/// Provisioned once per process: the tests run in parallel against one table.
static PROVISIONED: tokio::sync::OnceCell<()> = tokio::sync::OnceCell::const_new();

async fn executor() -> Option<Executor> {
    let url = fraiseql_test_support::try_database_url()?;
    let adapter = PostgresAdapter::new(&url).await.unwrap();
    PROVISIONED.get_or_init(|| provision(&adapter)).await;
    let mut schema = CompiledSchema::new();
    schema.fact_tables.insert(TABLE.to_string(), fact_table());
    Some(Executor::new(schema, Arc::new(adapter)))
}

async fn provision(adapter: &PostgresAdapter) {
    for ddl in [
        format!("DROP TABLE IF EXISTS {TABLE}"),
        format!("CREATE TABLE {TABLE} (id bigint, qty bigint, data jsonb)"),
        format!(
            "INSERT INTO {TABLE} VALUES (1, 5, '{{\"region\": \"r1\"}}'), \
             (2, 5, '{{\"region\": \"r2\"}}'), (3, 1, '{{\"region\": \"r3\"}}'), \
             (4, 2, '{{\"region\": \"r4\"}}')"
        ),
    ] {
        adapter.execute_raw_query(&ddl).await.unwrap();
    }
}

fn regions(response: &Value, key: &str) -> Vec<String> {
    response["data"][key]
        .as_array()
        .unwrap_or_else(|| panic!("{response}"))
        .iter()
        .map(|g| g["region"].as_str().unwrap().to_string())
        .collect()
}

#[tokio::test]
async fn an_aggregate_takes_its_arguments_inline_or_as_variables_alike() {
    let Some(executor) = executor().await else {
        eprintln!("skipping #1609: DATABASE_URL not set");
        return;
    };

    let inline = executor
        .execute(
            r#"{ inline_sale_aggregate(table: "tf_inline_sale", groupBy: {region: true},
                 aggregates: [{qty_sum: {}}], having: {qty_sum_gt: 1},
                 orderBy: {qty_sum: DESC, region: DESC}, limit: 2) { region qty_sum } }"#,
            None,
        )
        .await
        .unwrap();
    let variables = executor
        .execute(
            "{ inline_sale_aggregate { region qty_sum } }",
            Some(&json!({
                "table": TABLE, "groupBy": {"region": true}, "aggregates": [{"qty_sum": {}}],
                "having": {"qty_sum_gt": 1}, "orderBy": {"qty_sum": "DESC", "region": "DESC"},
                "limit": 2
            })),
        )
        .await
        .unwrap();
    // The written key order decides: region first sorts the groups by region alone.
    let region_first = executor
        .execute(
            r#"{ inline_sale_aggregate(table: "tf_inline_sale", groupBy: {region: true},
                 aggregates: [{qty_sum: {}}], orderBy: {region: ASC, qty_sum: DESC}) { region } }"#,
            None,
        )
        .await
        .unwrap();

    assert_eq!(regions(&inline, "inline_sale_aggregate"), ["r2", "r1"], "{inline}");
    assert_eq!(regions(&variables, "inline_sale_aggregate"), ["r2", "r1"], "{variables}");
    assert_eq!(
        regions(&region_first, "inline_sale_aggregate"),
        ["r1", "r2", "r3", "r4"],
        "{region_first}"
    );
}

/// The matcher's rule for every root field: a variable named like an argument wins over the
/// argument written inline.
#[tokio::test]
async fn a_variable_wins_over_the_same_argument_written_inline() {
    let Some(executor) = executor().await else {
        return;
    };

    let response = executor
        .execute(
            r#"{ inline_sale_aggregate(table: "tf_inline_sale", groupBy: {region: true},
                 aggregates: [{count: {}}], orderBy: {region: ASC}, limit: 3) { region } }"#,
            Some(&json!({ "limit": 1 })),
        )
        .await
        .unwrap();

    assert_eq!(regions(&response, "inline_sale_aggregate"), ["r1"], "{response}");
}

#[tokio::test]
async fn an_argument_no_parser_reads_is_refused_naming_it() {
    let Some(executor) = executor().await else {
        return;
    };

    let err = executor
        .execute(
            r#"{ inline_sale_aggregate(table: "tf_inline_sale", aggregates: [{count: {}}],
                 limt: 1) { count } }"#,
            None,
        )
        .await
        .expect_err("an unread argument is refused, never ignored")
        .to_string();

    assert!(err.contains("limt") && err.contains("limit"), "{err}");
}

#[tokio::test]
async fn a_window_takes_its_arguments_inline_or_as_variables_alike() {
    let Some(executor) = executor().await else {
        return;
    };

    let inline = executor
        .execute(
            r#"{ inline_sale_window(table: "tf_inline_sale",
                 select: [{type: "dimension", path: "region", alias: "region"}],
                 windows: [{function: {type: "row_number"}, alias: "position"}],
                 orderBy: [{field: "region", direction: "DESC"}], limit: 2) { region } }"#,
            None,
        )
        .await
        .unwrap();
    let variables = executor
        .execute(
            "{ inline_sale_window { region } }",
            Some(&json!({
                "table": TABLE,
                "select": [{"type": "dimension", "path": "region", "alias": "region"}],
                "windows": [{"function": {"type": "row_number"}, "alias": "position"}],
                "orderBy": [{"field": "region", "direction": "DESC"}],
                "limit": 2
            })),
        )
        .await
        .unwrap();

    assert_eq!(regions(&inline, "inline_sale_window"), ["r4", "r3"], "{inline}");
    assert_eq!(regions(&variables, "inline_sale_window"), ["r4", "r3"], "{variables}");
}
