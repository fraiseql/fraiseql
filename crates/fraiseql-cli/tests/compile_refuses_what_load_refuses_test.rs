#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable
//! `fraiseql compile` refuses a schema the server would refuse to load (ruling AF 4).
//!
//! Every load-time check (`CompiledSchema::finish_load`: duplicate names, type roles and
//! injects, subscription policies and filters, `requires_scope` without a `security`
//! section, fact-table links, relationships) ran only when a server started: the compiler
//! wrote an artifact it knew nothing wrong with, and the developer learned at deploy what
//! the compiler could have said. Two of those checks stand in for all of them here.
//!
//! **Execution engine:** in-memory (no database required)
//! **Infrastructure:** none
//! **Parallelism:** one test — `compile_to_schema` resolves its input against the process
//! working directory, so cases in separate `#[tokio::test]`s would race on `chdir`.

use fraiseql_cli::commands::compile::{CompileOptions, compile_to_schema};
use tempfile::TempDir;

/// `Sale` and a fact table read as it; `measures` is spliced in.
fn fact_table_schema(measures: &str) -> String {
    format!(
        r#"{{
  "types": [{{
    "name": "Sale",
    "fields": [
      {{"name": "id", "type": "Int", "nullable": false}},
      {{"name": "revenue", "type": "Float", "nullable": false}},
      {{"name": "category", "type": "String", "nullable": false}}
    ],
    "sql_source": "v_sale"
  }}],
  "fact_tables": [{{
    "table_name": "tf_sales",
    "type_name": "Sale",
    "measures": [{measures}],
    "dimensions": {{"name": "data", "paths": [
      {{"name": "category", "json_path": "data->>'category'", "data_type": "text"}}
    ]}},
    "denormalized_filters": []
  }}]
}}"#
    )
}

/// A scoped field and no `security` section: no role can grant the scope.
const SCOPE_WITHOUT_SECURITY: &str = r#"{
  "types": [{
    "name": "User",
    "fields": [
      {"name": "id", "type": "Int", "nullable": false},
      {"name": "salary", "type": "Int", "nullable": true, "requires_scope": "read:salary"}
    ],
    "sql_source": "v_user"
  }]
}"#;

/// Compile `schema_json` from a fresh temp dir, restoring the working directory.
async fn compile(schema_json: &str) -> anyhow::Result<()> {
    let dir = TempDir::new().expect("temp dir");
    std::fs::write(dir.path().join("schema.json"), schema_json).expect("write schema.json");
    let original = std::env::current_dir().expect("cwd");
    std::env::set_current_dir(dir.path()).expect("chdir into temp dir");
    let result = compile_to_schema(CompileOptions::new("schema.json")).await;
    std::env::set_current_dir(original).expect("restore cwd");
    result.map(|_| ())
}

#[tokio::test]
async fn compile_refuses_what_the_server_would_refuse_to_load() {
    // A fact table declaring `cost`, which `Sale` lacks: the link cannot gate it.
    let incomplete = fact_table_schema(
        r#"{"name": "revenue", "sql_type": "numeric", "nullable": false},
           {"name": "cost", "sql_type": "numeric", "nullable": true}"#,
    );
    let err = compile(&incomplete)
        .await
        .expect_err("an incomplete fact-table link must not compile");
    let msg = format!("{err:#}");
    assert!(msg.contains("cost"), "names the column the type lacks: {msg}");

    let err = compile(SCOPE_WITHOUT_SECURITY)
        .await
        .expect_err("`requires_scope` without a `security` section must not compile");
    let msg = format!("{err:#}");
    assert!(msg.contains("read:salary"), "names the scope: {msg}");

    // Control: the complete link compiles.
    let complete =
        fact_table_schema(r#"{"name": "revenue", "sql_type": "numeric", "nullable": false}"#);
    compile(&complete).await.expect("a complete fact-table link compiles");
}
