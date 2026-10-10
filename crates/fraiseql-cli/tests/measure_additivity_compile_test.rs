#![allow(clippy::unwrap_used, clippy::panic)] // Reason: test code, panics are acceptable
//! #1459: a fact-table measure declares how it aggregates over time, and the compiler carries
//! it to the artifact or refuses a declaration the runtime could not honour.
//!
//! A balance or a stock level is **semi-additive**: summed across accounts, never across days.
//! Without the declaration the planner summed every day's row. `semi_additive` and `delta`
//! name the time column they reduce over (`over`, a denormalized time column) and the entity
//! the value belongs to (`entity`, denormalized columns); a declaration missing either, or
//! naming a column that is not there or not a time, cannot be planned and fails the compile.
//!
//! **Execution engine:** in-memory (no database: the refusal is the compiler's).

use fraiseql_cli::commands::compile::{CompileOptions, compile_to_schema};
use fraiseql_core::compiler::fact_table::{Additivity, SemiAdditiveReduction};
use serde_json::{Value, json};

/// `tf_account_day`: one row per account and day the balance changed.
fn schema_with(additivity: Option<Value>) -> Value {
    let mut balance =
        json!({ "name": "closing_balance", "sql_type": "numeric", "nullable": false });
    if let Some(additivity) = additivity {
        balance["additivity"] = additivity;
    }
    json!({
        "fact_tables": [{
            "table_name": "tf_account_day",
            "measures": [
                balance,
                { "name": "deposits", "sql_type": "numeric", "nullable": false }
            ],
            "dimensions": { "name": "data", "paths": [] },
            "denormalized_filters": [
                { "name": "account_id", "sql_type": "bigint", "indexed": true },
                { "name": "day", "sql_type": "date", "indexed": true },
                { "name": "branch", "sql_type": "text", "indexed": false }
            ]
        }]
    })
}

/// `fraiseql compile schema.json`: the conversion, then the load checks it runs on its own
/// artifact.
fn compile(schema: Value) -> Result<fraiseql_core::schema::CompiledSchema, String> {
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

fn additivity_of(schema: &fraiseql_core::schema::CompiledSchema, measure: &str) -> Additivity {
    schema.fact_tables["tf_account_day"]
        .measures
        .iter()
        .find(|m| m.name == measure)
        .unwrap()
        .additivity
        .clone()
}

#[test]
fn a_declared_semi_additive_measure_reaches_the_artifact() {
    let schema = compile(schema_with(Some(json!({
        "kind": "semi_additive", "over": "day", "using": "last", "entity": ["account_id"]
    }))))
    .unwrap();
    assert_eq!(
        additivity_of(&schema, "closing_balance"),
        Additivity::SemiAdditive {
            over:   "day".to_string(),
            using:  SemiAdditiveReduction::Last,
            entity: vec!["account_id".to_string()],
        }
    );
    // An undeclared measure is additive, as every measure was.
    assert_eq!(additivity_of(&schema, "deposits"), Additivity::Additive);
}

#[test]
fn delta_and_non_additive_reach_the_artifact() {
    let schema = compile(schema_with(Some(json!({
        "kind": "delta", "over": "day", "entity": ["account_id"]
    }))))
    .unwrap();
    assert_eq!(
        additivity_of(&schema, "closing_balance"),
        Additivity::Delta {
            over:   "day".to_string(),
            entity: vec!["account_id".to_string()],
        }
    );
    let schema = compile(schema_with(Some(json!({ "kind": "non_additive" })))).unwrap();
    assert_eq!(additivity_of(&schema, "closing_balance"), Additivity::NonAdditive);
}

/// Each declaration the planner could not honour fails the compile, naming the measure and
/// the column at fault.
#[test]
fn a_declaration_that_cannot_be_planned_fails_the_compile() {
    for (additivity, names) in [
        // `over` must be a declared time column.
        (
            json!({ "kind": "semi_additive", "over": "nope", "using": "last", "entity": ["account_id"] }),
            "nope",
        ),
        (
            json!({ "kind": "semi_additive", "over": "branch", "using": "last", "entity": ["account_id"] }),
            "branch",
        ),
        // `entity` must name declared columns, at least one.
        (
            json!({ "kind": "semi_additive", "over": "day", "using": "last", "entity": [] }),
            "entity",
        ),
        (json!({ "kind": "delta", "over": "day", "entity": ["nope"] }), "nope"),
    ] {
        let err = match compile(schema_with(Some(additivity.clone()))) {
            Ok(_) => panic!("{additivity} compiled"),
            Err(e) => e,
        };
        assert!(
            err.contains("closing_balance") && err.contains(names),
            "{additivity}: names the measure and `{names}`: {err}"
        );
    }
    // A missing `over` or `using` is a malformed declaration.
    let err =
        compile(schema_with(Some(json!({ "kind": "semi_additive", "entity": ["account_id"] }))))
            .expect_err("a semi-additive measure without `over` must not compile");
    assert!(err.contains("over"), "{err}");
}
