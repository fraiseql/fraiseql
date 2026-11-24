//! An aggregate's `orderBy` key puts its NULLs first or last, as its direction names
//! (`DESC_NULLS_LAST`, …): a computed aggregate that is NULL for a group, and a group key that
//! is NULL. Each order is compared with the one PostgreSQL gives the same `ORDER BY … NULLS …`.
//!
//! **Parallelism:** creates and drops its own `tf_aggregate_nulls`.

use std::{collections::HashMap, sync::Arc};

use fraiseql_core::{
    compiler::fact_table::{
        Additivity, DimensionColumn, DimensionPath, FactRowScope, FactTableMetadata, MeasureColumn,
        SqlType,
    },
    db::{DatabaseAdapter, postgres::PostgresAdapter},
    runtime::Executor,
    schema::CompiledSchema,
};
use serde_json::{Value, json};

const TABLE: &str = "tf_aggregate_nulls";
const ROOT: &str = "aggregate_nulls_aggregate";

/// `(revenue, team)`: team `b` sums to NULL, and two rows have no team.
const ROWS: [(Option<i64>, Option<&str>); 7] = [
    (Some(10), Some("a")),
    (Some(20), Some("a")),
    (None, Some("b")),
    (None, Some("b")),
    (Some(5), Some("c")),
    (Some(7), None),
    (None, None),
];

fn metadata() -> FactTableMetadata {
    FactTableMetadata {
        table_name:               TABLE.to_string(),
        type_name:                None,
        row_scope:                FactRowScope::Server,
        inject_params:            indexmap::IndexMap::default(),
        measures:                 vec![MeasureColumn {
            name:       "revenue".to_string(),
            sql_type:   SqlType::Int,
            nullable:   true,
            additivity: Additivity::Additive,
            column:     None,
        }],
        dimensions:               DimensionColumn {
            name:  "data".to_string(),
            paths: vec![DimensionPath {
                name:      "team".to_string(),
                json_path: "data->>'team'".to_string(),
                data_type: "string".to_string(),
            }],
        },
        denormalized_filters:     vec![],
        calendar_dimensions:      vec![],
        grain:                    None,
        native_dimension_mapping: HashMap::new(),
    }
}

async fn setup() -> Option<(PostgresAdapter, Executor)> {
    let Some(url) = fraiseql_test_support::try_database_url() else {
        eprintln!("skipping: DATABASE_URL not set");
        return None;
    };
    let adapter = PostgresAdapter::new(&url).await.unwrap();
    let values: Vec<String> = ROWS
        .iter()
        .map(|(revenue, team)| {
            let revenue = revenue.map_or_else(|| "NULL".to_string(), |r| r.to_string());
            let data = team.map_or_else(|| "{}".to_string(), |t| format!(r#"{{"team": "{t}"}}"#));
            format!("({revenue}, '{data}')")
        })
        .collect();
    for ddl in [
        format!("DROP TABLE IF EXISTS {TABLE}"),
        format!("CREATE TABLE {TABLE} (revenue int, data jsonb)"),
        format!("INSERT INTO {TABLE} VALUES {}", values.join(", ")),
    ] {
        adapter.execute_raw_query(&ddl).await.unwrap();
    }
    let mut schema = CompiledSchema::new();
    schema.add_fact_table(TABLE.to_string(), metadata());
    schema.build_indexes();
    let executor = Executor::new(schema, Arc::new(PostgresAdapter::new(&url).await.unwrap()));
    Some((adapter, executor))
}

/// `(team, revenue_sum)` of each group, in the order served; a sum read as text or a number.
fn groups(rows: &[Value]) -> Vec<(Option<String>, Option<i64>)> {
    rows.iter()
        .map(|row| {
            let sum = &row["revenue_sum"];
            (
                row["team"].as_str().map(str::to_string),
                sum.as_i64().or_else(|| sum.as_str().and_then(|s| s.parse().ok())),
            )
        })
        .collect()
}

#[tokio::test]
async fn an_aggregate_puts_its_null_aggregates_and_null_keys_where_the_direction_asks() {
    let Some((adapter, executor)) = setup().await else {
        return;
    };
    for (order, sql_order) in [
        (json!({ "revenue_sum": "DESC_NULLS_LAST" }), "2 DESC NULLS LAST"),
        (json!({ "revenue_sum": "ASC_NULLS_FIRST" }), "2 ASC NULLS FIRST"),
        (json!({ "team": "ASC_NULLS_FIRST" }), "1 ASC NULLS FIRST"),
        (json!({ "team": "DESC_NULLS_LAST" }), "1 DESC NULLS LAST"),
    ] {
        let query = json!({ "groupBy": { "team": true },
                            "aggregates": [{ "revenue_sum": true }],
                            "orderBy": order });
        let response = executor
            .execute_aggregate_query(&query, ROOT, &metadata())
            .await
            .unwrap_or_else(|e| panic!("{query}: {e}"));
        let served =
            groups(response["data"][ROOT].as_array().unwrap_or_else(|| panic!("{response}")));
        let expected: Vec<(Option<String>, Option<i64>)> = adapter
            .execute_raw_query(&format!(
                "SELECT data->>'team' AS team, SUM(revenue) AS revenue_sum FROM {TABLE} \
                 GROUP BY 1 ORDER BY {sql_order}"
            ))
            .await
            .unwrap()
            .iter()
            .map(|row| groups(&[serde_json::to_value(row).unwrap()]).remove(0))
            .collect();
        assert_eq!(served, expected, "{order}");
    }
}
