#![allow(clippy::unwrap_used, clippy::panic)] // Reason: test code, panics are acceptable
//! A fact-table measure stored under another column declares it as `column`, beside its
//! `sql_type`, and the compiler carries both to the artifact: the measure is published by
//! its name and typed by its `sql_type`, as every measure is. The `native_measures` section
//! that mapped a name to a column without a type is refused by name, and so is a `column`
//! that is not one SQL identifier.
//!
//! **Execution engine:** in-memory (no database: the refusals are the compiler's).

use fraiseql_cli::commands::compile::{CompileOptions, compile_to_schema};
use serde_json::{Value, json};

/// `tf_daily_volume`: a pre-aggregated view storing `volume` as `volume_total`.
fn schema_with(volume: &Value, extra: Option<(&str, Value)>) -> Value {
    let mut table = json!({
        "table_name": "tf_daily_volume",
        "measures": [volume],
        "dimensions": { "name": "data", "paths": [] },
        "denormalized_filters": [{ "name": "day", "sql_type": "date", "indexed": true }]
    });
    if let Some((key, value)) = extra {
        table[key] = value;
    }
    json!({ "fact_tables": [table] })
}

fn volume(column: &str) -> Value {
    json!({ "name": "volume", "sql_type": "numeric", "nullable": false, "column": column })
}

/// `fraiseql compile schema.json`: the conversion, then the load checks it runs on its own
/// artifact.
fn compile(schema: &Value) -> Result<fraiseql_core::schema::CompiledSchema, String> {
    let dir = tempfile::TempDir::new().unwrap();
    let path = dir.path().join("schema.json");
    std::fs::write(&path, schema.to_string()).unwrap();
    let runtime = tokio::runtime::Runtime::new().unwrap();
    runtime
        .block_on(compile_to_schema(CompileOptions {
            skip_hash: true,
            ..CompileOptions::new(path.to_str().unwrap())
        }))
        .map(|(compiled, _)| compiled.schema)
        .map_err(|e| format!("{e:#}"))
}

#[test]
fn a_measure_column_reaches_the_artifact_with_its_type() {
    let schema = compile(&schema_with(&volume("volume_total"), None)).unwrap();
    let measure = schema.fact_tables["tf_daily_volume"].measure("volume").unwrap();
    assert_eq!(measure.column.as_deref(), Some("volume_total"));
    assert_eq!(measure.sql_type, fraiseql_core::compiler::fact_table::SqlType::Decimal);
}

#[test]
fn a_native_measures_section_is_refused_naming_measure_column() {
    let measure = json!({ "name": "volume", "sql_type": "numeric", "nullable": false });
    let err = compile(&schema_with(
        &measure,
        Some(("native_measures", json!({ "volume": "volume_total" }))),
    ))
    .expect_err("a removed section must not compile");
    assert!(err.contains("`native_measures`"), "{err}");
    assert!(err.contains("`measures[].column`"), "{err}");
    assert!(err.contains("tf_daily_volume"), "{err}");
}

/// The column is interpolated, quoted, into every read of the measure: one identifier.
#[test]
fn a_measure_column_that_is_not_one_identifier_does_not_compile() {
    for hostile in [
        r#"volume_total") FROM pg_authid; --"#,
        "volume total",
        "public.volume",
    ] {
        let err = compile(&schema_with(&volume(hostile), None))
            .expect_err("a column that is not an identifier must not compile");
        assert!(
            err.contains("measure `volume`") && err.contains("not a SQL column name"),
            "{err}"
        );
    }
}
