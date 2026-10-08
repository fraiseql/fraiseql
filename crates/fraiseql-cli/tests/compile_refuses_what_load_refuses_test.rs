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

/// A fact table read as no type, with `filters` and `mapping` spliced in.
fn unlinked_fact_table(filters: &str, mapping: &str) -> String {
    format!(
        r#"{{
  "fact_tables": [{{
    "table_name": "tf_sales",
    "measures": [{{"name": "revenue", "sql_type": "numeric", "nullable": false}}],
    "dimensions": {{"name": "data", "paths": []}},
    "denormalized_filters": [{filters}],
    "native_dimension_mapping": {mapping}
  }}]
}}"#
    )
}

/// A fact table whose dimension column declares `paths`.
fn fact_table_with_paths(paths: &str) -> String {
    format!(
        r#"{{
  "fact_tables": [{{
    "table_name": "tf_sales",
    "measures": [{{"name": "revenue", "sql_type": "numeric", "nullable": false}}],
    "dimensions": {{"name": "data", "paths": [{paths}]}},
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

    // #1231: one dimension declared twice, in two vocabularies that disagree: a filter
    // column named `category`, and a mapping of `category` to another column. A request
    // would read one or the other depending on which path parsed it.
    let conflicting = unlinked_fact_table(
        r#"{"name": "category", "sql_type": "text", "indexed": true}"#,
        r#"{"category": "category_id"}"#,
    );
    let err = compile(&conflicting)
        .await
        .expect_err("a dimension mapped to one column and filtered as another must not compile");
    let msg = format!("{err:#}");
    assert!(
        msg.contains("native_dimension_mapping") && msg.contains("denormalized_filters"),
        "names both declarations: {msg}"
    );
    assert!(msg.contains("category_id"), "names the mapped column: {msg}");

    // #1231: two mapping keys for one dimension in two casings, to different columns.
    let twice = unlinked_fact_table("", r#"{"itemCategory": "a_col", "item_category": "b_col"}"#);
    let err = compile(&twice)
        .await
        .expect_err("one dimension mapped to two columns must not compile");
    let msg = format!("{err:#}");
    assert!(
        msg.contains("itemCategory") && msg.contains("item_category"),
        "names both keys: {msg}"
    );

    // #1517: a mapping onto a column the fact table does not declare. Nothing gives the
    // column a type (the cast a filter on it binds with), and nothing says it exists.
    let undeclared = unlinked_fact_table("", r#"{"category": "category_id"}"#);
    let err = compile(&undeclared)
        .await
        .expect_err("a mapping onto an undeclared column must not compile");
    let msg = format!("{err:#}");
    assert!(
        msg.contains("category_id") && msg.contains("denormalized_filters"),
        "names the column and where to declare it: {msg}"
    );

    // Control: a mapping that agrees with the filter columns compiles.
    let agreeing = unlinked_fact_table(
        r#"{"name": "category_id", "sql_type": "int", "indexed": true}"#,
        r#"{"category": "category_id"}"#,
    );
    compile(&agreeing)
        .await
        .expect("a mapping onto a declared filter column compiles");

    // #1517: a declared dimension path is read at its `json_path`, so the path must be one
    // the runtime can read: the dimensions column, `->'key'` steps, one final `->>'key'`.
    let expression = fact_table_with_paths(
        r#"{"name": "amount", "json_path": "(data->>'amount')::int", "data_type": "int"}"#,
    );
    let err = compile(&expression)
        .await
        .expect_err("a json_path the runtime cannot read must not compile");
    let msg = format!("{err:#}");
    assert!(
        msg.contains("amount") && msg.contains("(data->>'amount')::int"),
        "names the path and its json_path: {msg}"
    );

    // Control: a nested path of the readable shape compiles.
    let nested = fact_table_with_paths(
        r#"{"name": "machine_model", "json_path": "data->'machine'->>'model'", "data_type": "text"}"#,
    );
    compile(&nested).await.expect("a nested readable json_path compiles");

    // #1513: `localized` is a String stored as a locale map, resolved through [locale].
    let localized = |field_type: &str| {
        format!(
            r#"{{"types": [{{"name": "Product", "sql_source": "tv_product", "fields": [
                {{"name": "id", "type": "ID", "nullable": false}},
                {{"name": "name", "type": "{field_type}", "nullable": true, "localized": true}}
            ]}}]}}"#
        )
    };
    let err = compile(&localized("Int")).await.expect_err("a localized Int must not compile");
    assert!(format!("{err:#}").contains("`Product.name` is localized but is not a String"));
    let err = compile(&localized("String"))
        .await
        .expect_err("a localized field without [locale] must not compile");
    assert!(format!("{err:#}").contains("declares no [locale]"), "{err:#}");

    // Control: the complete link compiles.
    let complete =
        fact_table_schema(r#"{"name": "revenue", "sql_type": "numeric", "nullable": false}"#);
    compile(&complete).await.expect("a complete fact-table link compiles");
}
