#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable
//! `fraiseql compile` carries a rolled-up fact table's `grain.edges_from` into the artifact,
//! and refuses a link whose finer table is not the same table at a finer grain.
//!
//! **Execution engine:** in-memory (no database required)
//! **Infrastructure:** none
//! **Parallelism:** one test — `compile_to_schema` resolves its input against the process
//! working directory, so cases in separate `#[tokio::test]`s would race on `chdir`.

use fraiseql_cli::commands::compile::{CompileOptions, compile_to_schema};
use fraiseql_core::schema::CompiledSchema;
use serde_json::{Value, json};
use tempfile::TempDir;

/// A fact table over `day` and `region`, measuring `revenue`, rolled up to `bucket`.
fn table(name: &str, bucket: &str) -> Value {
    json!({
        "table_name": name,
        "measures": [{"name": "revenue", "sql_type": "bigint", "nullable": false}],
        "dimensions": {"name": "data", "paths": []},
        "denormalized_filters": [
            {"name": "day", "sql_type": "date", "indexed": true},
            {"name": "booked", "sql_type": "date", "indexed": true},
            {"name": "region", "sql_type": "text", "indexed": true}
        ],
        "grain": {"column": "day", "bucket": bucket}
    })
}

/// `tf_sales_month` linked to `tf_sales_day`, each table changed by `edit` first.
fn schema(edit: impl FnOnce(&mut Value, &mut Value)) -> String {
    let mut month = table("tf_sales_month", "month");
    month["grain"]["edges_from"] = json!("tf_sales_day");
    let mut day = table("tf_sales_day", "day");
    edit(&mut month, &mut day);
    json!({"types": [], "queries": [], "fact_tables": [month, day]}).to_string()
}

/// Compile `schema_json` from a fresh temp dir, restoring the working directory.
async fn compile(schema_json: &str) -> anyhow::Result<CompiledSchema> {
    let dir = TempDir::new().expect("temp dir");
    std::fs::write(dir.path().join("schema.json"), schema_json).expect("write schema.json");
    let original = std::env::current_dir().expect("cwd");
    std::env::set_current_dir(dir.path()).expect("chdir into temp dir");
    let result = compile_to_schema(CompileOptions::new("schema.json")).await;
    std::env::set_current_dir(original).expect("restore cwd");
    result.map(|(artifact, _)| artifact.schema)
}

#[tokio::test]
async fn compile_carries_a_linked_grain_and_refuses_one_that_is_not_the_same_table() {
    let compiled = compile(&schema(|_, _| {})).await.expect("a linked grain compiles");
    let grain = compiled.get_fact_table("tf_sales_month").unwrap().grain.clone().unwrap();
    assert_eq!(grain.edges_from.as_deref(), Some("tf_sales_day"));

    let edges = "fact table `tf_sales_month`: `grain.edges_from` `tf_sales_day`";
    let cases: Vec<(String, String)> = vec![
        (
            schema(|month, _| month["grain"]["edges_from"] = json!("tf_sales_month")),
            "`grain.edges_from` `tf_sales_month` names the table itself".to_string(),
        ),
        (
            schema(|month, _| month["grain"]["edges_from"] = json!("tf_sales_week")),
            "`grain.edges_from` `tf_sales_week` is not a fact table of the schema".to_string(),
        ),
        (
            schema(|_, day| day["grain"] = Value::Null),
            format!("{edges} declares no `grain`"),
        ),
        (
            schema(|_, day| day["grain"]["column"] = json!("booked")),
            format!("{edges} declares its grain on `booked`, and this table on `day`"),
        ),
        (
            schema(|_, day| day["grain"]["bucket"] = json!("month")),
            format!(
                "{edges} holds one row per month, which is not a finer period nesting in a month"
            ),
        ),
        (
            schema(|_, day| day["grain"]["bucket"] = json!("week")),
            format!(
                "{edges} holds one row per week, which is not a finer period nesting in a month"
            ),
        ),
        (
            schema(|_, day| day["grain"]["edges_from"] = json!("tf_sales_month")),
            format!("{edges} links edges of its own (`tf_sales_month`)"),
        ),
        (
            schema(|_, day| day["measures"][0]["nullable"] = json!(true)),
            format!("{edges} declares measure `revenue` with another type"),
        ),
        (
            schema(|_, day| day["measures"][0]["sql_type"] = json!("integer")),
            format!("{edges} declares measure `revenue` with another type"),
        ),
        (
            schema(|_, day| day["measures"][0]["additivity"] = json!({"kind": "non_additive"})),
            format!("{edges} declares measure `revenue` with another type"),
        ),
        (
            schema(|_, day| {
                day["measures"]
                    .as_array_mut()
                    .unwrap()
                    .push(json!({"name": "cost", "sql_type": "bigint", "nullable": false}));
            }),
            format!("{edges} measure `cost` is declared on only one of the two tables"),
        ),
        (
            schema(|month, _| {
                month["denormalized_filters"].as_array_mut().unwrap().remove(2);
            }),
            format!(
                "{edges} denormalized filter `region` is declared on only one of the two tables"
            ),
        ),
        (
            schema(|_, day| day["denormalized_filters"][2]["indexed"] = json!(false)),
            format!("{edges} declares denormalized filter `region` with another definition"),
        ),
        (
            schema(|_, day| day["dimensions"]["name"] = json!("attrs")),
            format!("{edges} declares other `dimensions`"),
        ),
        (
            schema(|_, day| day["native_dimension_mapping"] = json!({"category": "region"})),
            format!("{edges} declares another `native_dimension_mapping`"),
        ),
        (
            schema(|_, day| day["row_scope"] = json!("relation")),
            format!("{edges} is read under other gates than `tf_sales_month`"),
        ),
    ];
    for (schema_json, expected) in cases {
        let err = compile(&schema_json).await.expect_err("an unanswerable link is refused");
        let msg = format!("{err:#}");
        assert!(msg.contains(&expected), "expected {expected:?} in: {msg}");
    }
}
