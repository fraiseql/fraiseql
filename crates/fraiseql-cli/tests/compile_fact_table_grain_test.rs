#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable
//! `fraiseql compile` carries a fact table's declared `grain` into the artifact, and
//! refuses one the runtime could not check requests against: an unknown bucket or key, a
//! column that is not a declared `date` filter.
//!
//! **Execution engine:** in-memory (no database required)
//! **Infrastructure:** none
//! **Parallelism:** one test — `compile_to_schema` resolves its input against the process
//! working directory, so cases in separate `#[tokio::test]`s would race on `chdir`.

use fraiseql_cli::commands::compile::{CompileOptions, compile_to_schema};
use fraiseql_core::{
    compiler::fact_table::{FactGrain, GrainBucket},
    schema::CompiledSchema,
};
use serde_json::{Value, json};
use tempfile::TempDir;

/// `tf_sales` with a `date` column `day`, a `timestamptz` column `at`, and `grain`.
fn schema(grain: &Value) -> String {
    json!({
        "types": [],
        "queries": [],
        "fact_tables": [{
            "table_name": "tf_sales",
            "measures": [{"name": "revenue", "sql_type": "numeric", "nullable": false}],
            "dimensions": {"name": "data", "paths": []},
            "denormalized_filters": [
                {"name": "day", "sql_type": "date", "indexed": true},
                {"name": "at", "sql_type": "timestamptz", "indexed": true}
            ],
            "grain": grain
        }]
    })
    .to_string()
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
async fn compile_carries_a_grain_and_refuses_one_it_cannot_enforce() {
    let compiled = compile(&schema(&json!({"column": "day", "bucket": "half_month"})))
        .await
        .expect("a grain on a date column compiles");
    assert_eq!(
        compiled.get_fact_table("tf_sales").unwrap().grain,
        Some(FactGrain {
            column:     "day".to_string(),
            bucket:     GrainBucket::HalfMonth,
            edges_from: None,
        })
    );

    let undeclared = compile(&schema(&Value::Null)).await.expect("no grain compiles");
    assert_eq!(undeclared.get_fact_table("tf_sales").unwrap().grain, None);

    for (grain, expected) in [
        (json!({"column": "day", "bucket": "fortnight"}), "fortnight"),
        (json!({"column": "day", "bucket": "hour"}), "hour"),
        (json!({"column": "day", "bucket": "month", "unit": "month"}), "unit"),
        (json!({"column": "at", "bucket": "month"}), "`grain` is for date columns only"),
        (json!({"column": "booked_on", "bucket": "month"}), "`booked_on` is not one of"),
    ] {
        let err = compile(&schema(&grain)).await.expect_err("an unenforceable grain is refused");
        let msg = format!("{err:#}");
        assert!(msg.contains(expected), "{grain}: {msg}");
    }
}
