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
    let schema = compile(&schema_with(Some(json!({
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
    let schema = compile(&schema_with(Some(json!({
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
    let schema = compile(&schema_with(Some(json!({ "kind": "non_additive" })))).unwrap();
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
        let Err(err) = compile(&schema_with(Some(additivity.clone()))) else {
            panic!("{additivity} compiled")
        };
        assert!(
            err.contains("closing_balance") && err.contains(names),
            "{additivity}: names the measure and `{names}`: {err}"
        );
    }
    // A missing `over` or `using` is a malformed declaration.
    let err =
        compile(&schema_with(Some(json!({ "kind": "semi_additive", "entity": ["account_id"] }))))
            .expect_err("a semi-additive measure without `over` must not compile");
    assert!(err.contains("over"), "{err}");
}

/// A `fraiseql.toml` whose `[validation]` is `validation`.
fn toml_with_validation(validation: &str) -> String {
    format!(
        r#"
[schema]
name = "additivity-1459"
version = "1.0.0"
database_target = "postgresql"

[validation]
{validation}

[types.Account]
sql_source = "v_account"
fields.id = {{ type = "Int" }}

[queries.accounts]
return_type = "Account"
return_array = true
sql_source = "v_account"
"#
    )
}

fn compile_toml(document: &str) -> Result<fraiseql_core::schema::CompiledSchema, String> {
    let dir = tempfile::TempDir::new().unwrap();
    let path = dir.path().join("fraiseql.toml");
    std::fs::write(&path, document).unwrap();
    tokio::runtime::Runtime::new()
        .unwrap()
        .block_on(compile_to_schema(CompileOptions {
            skip_hash: true,
            ..CompileOptions::new(path.to_str().unwrap())
        }))
        .map(|(compiled, _)| compiled.schema)
        .map_err(|e| format!("{e:#}"))
}

/// The cell bound a carried-forward aggregate may read reaches the artifact; 0, which would
/// refuse every such aggregate, is refused at compile and, in a hand-written artifact, at
/// load.
#[test]
fn the_semi_additive_cell_bound_is_carried_and_zero_is_refused() {
    let schema = compile_toml(&toml_with_validation("max_semi_additive_cells = 5")).unwrap();
    assert_eq!(
        schema.validation_config.as_ref().and_then(|v| v.max_semi_additive_cells),
        Some(5)
    );

    let err = compile_toml(&toml_with_validation("max_semi_additive_cells = 0"))
        .expect_err("a bound of 0 must not compile");
    assert!(err.contains("max_semi_additive_cells = 0"), "{err}");

    let mut artifact: Value = serde_json::from_str(&schema.to_json().unwrap()).unwrap();
    artifact["validation_config"]["max_semi_additive_cells"] = json!(0);
    let err = fraiseql_core::schema::CompiledSchema::from_json(&artifact.to_string(), false)
        .expect_err("a bound of 0 must not load")
        .to_string();
    assert!(err.contains("max_semi_additive_cells"), "{err}");
}
