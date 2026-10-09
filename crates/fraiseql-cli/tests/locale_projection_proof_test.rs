#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable
//! #1512's projection proof at the DDL emitters: `[locale]` reaches no stored projection.
//!
//! The request locale shapes reads only (an `ORDER BY … COLLATE`, the `fraiseql.locale`
//! setting on read transactions). Nothing the compiler emits for the database, which is what a
//! view, table or capture trigger is built from, may depend on it. So the same project
//! compiled with and without `[locale]` must emit byte-identical DDL, and the compiled schemas
//! must differ (the input is not vacuous).
//!
//! **Execution engine:** in-memory (no database required) · **Infrastructure:** none ·
//! **Parallelism:** writes only to its own temp dirs.

use fraiseql_cli::commands::{
    compile::{CompileOptions, compile_to_schema, emit_ddl_to_dir, localized_index_report_text},
    generate_capture_triggers::build_ddl,
};
use tempfile::TempDir;

const SCHEMA_JSON: &str = r#"{
  "types": [{
    "name": "Product",
    "fields": [
      {"name": "id", "type": "ID", "nullable": false},
      {"name": "name", "type": "String", "nullable": false},
      {"name": "price", "type": "Float", "nullable": true}
    ],
    "sql_source": "tv_product",
    "subscribable_tables": ["tb_product"]
  }],
  "queries": [{
    "name": "products", "return_type": "Product", "returns_list": true,
    "sql_source": "tv_product", "nullable": false, "arguments": []
  }],
  "mutations": [], "subscriptions": [], "version": "2.0.0"
}"#;

const LOCALE: &str = "\n[locale]\ndefault = \"en-US\"\nallowed = [\"en-US\", \"fr-FR\"]\n";

/// The compiled schema, the files `--emit-ddl` writes (name → bytes), and the capture-trigger DDL,
/// for `schema` compiled with `locale`.
async fn emitted_from(
    schema: &str,
    locale: &str,
) -> (serde_json::Value, Vec<(String, Vec<u8>)>, String) {
    let dir = TempDir::new().unwrap();
    std::fs::write(dir.path().join("schema.json"), schema).unwrap();
    std::fs::write(
        dir.path().join("fraiseql.toml"),
        format!("[project]\nname = \"p\"\n\n[fraiseql]\nschema_file = \"schema.json\"\n{locale}"),
    )
    .unwrap();
    let input = dir.path().join("schema.json");
    let (artifact, _) =
        compile_to_schema(CompileOptions::new(input.to_str().unwrap())).await.unwrap();
    let ddl_dir = dir.path().join("ddl");
    emit_ddl_to_dir(&artifact.schema, ddl_dir.to_str().unwrap()).unwrap();
    let mut files: Vec<(String, Vec<u8>)> = std::fs::read_dir(&ddl_dir)
        .unwrap()
        .map(|e| {
            let e = e.unwrap();
            (e.file_name().to_string_lossy().into_owned(), std::fs::read(e.path()).unwrap())
        })
        .collect();
    files.sort();
    let triggers = build_ddl(&artifact.schema, true);
    (serde_json::to_value(&artifact.schema).unwrap(), files, triggers)
}

async fn emitted(locale: &str) -> (serde_json::Value, Vec<(String, Vec<u8>)>, String) {
    emitted_from(SCHEMA_JSON, locale).await
}

/// #1513's projection proof: a localized field's chain, sibling and literals live in reads only.
/// The project with `name` localized (and the `[locale]` that requires) emits the same DDL as
/// without either: a view, table or trigger built from it stores the whole locale map.
#[tokio::test]
async fn the_ddl_emitters_ignore_localized_fields() {
    let localized = SCHEMA_JSON.replace(
        r#"{"name": "name", "type": "String", "nullable": false}"#,
        r#"{"name": "name", "type": "String", "nullable": false, "localized": true}"#,
    );
    assert_ne!(localized, SCHEMA_JSON, "the fixture marks `name` localized");
    let (plain_schema, plain_ddl, plain_triggers) = emitted("").await;
    let (localized_schema, localized_ddl, localized_triggers) =
        emitted_from(&localized, LOCALE).await;

    assert_ne!(plain_schema, localized_schema, "`localized` reached the compiled schema");
    assert_eq!(plain_ddl, localized_ddl, "--emit-ddl output is the same for a localized field");
    assert_eq!(plain_triggers, localized_triggers, "capture triggers are the same");
}

#[tokio::test]
async fn the_ddl_emitters_ignore_the_locale() {
    let (plain_schema, plain_ddl, plain_triggers) = emitted("").await;
    let (localized_schema, localized_ddl, localized_triggers) = emitted(LOCALE).await;

    assert_ne!(plain_schema, localized_schema, "the locale reached the compiled schema");
    assert!(!plain_ddl.is_empty() && !plain_triggers.is_empty(), "the emitters emitted");
    assert_eq!(plain_ddl, localized_ddl, "--emit-ddl output is the same with [locale]");
    assert_eq!(
        plain_triggers, localized_triggers,
        "capture triggers are the same with [locale]"
    );
}

/// #1513: `compile` prints one index per localized field and allowed locale, each the DDL the
/// schema reports (the statement a filter or sort is planned against, proven on PostgreSQL by
/// `fraiseql-core`'s `locale_localized_index`). A project with no localized field prints none.
#[tokio::test]
async fn compile_prints_the_localized_index_report() {
    let localized = SCHEMA_JSON.replace(
        r#"{"name": "name", "type": "String", "nullable": false}"#,
        r#"{"name": "name", "type": "String", "nullable": false, "localized": true}"#,
    );
    let dir = TempDir::new().unwrap();
    std::fs::write(dir.path().join("schema.json"), &localized).unwrap();
    std::fs::write(
        dir.path().join("fraiseql.toml"),
        format!("[project]\nname = \"p\"\n\n[fraiseql]\nschema_file = \"schema.json\"\n{LOCALE}"),
    )
    .unwrap();
    let input = dir.path().join("schema.json");
    let (artifact, _) =
        compile_to_schema(CompileOptions::new(input.to_str().unwrap())).await.unwrap();
    let report = artifact.schema.localized_index_report();
    assert_eq!(report.len(), 2, "one per allowed locale: {report:?}");
    let text = localized_index_report_text(&artifact.schema);
    // What `compile` prints is what the loaded schema the server plans with reports.
    let loaded = fraiseql_core::schema::CompiledSchema::from_json(
        &serde_json::to_string(&artifact.schema).unwrap(),
        false,
    )
    .unwrap();
    assert_eq!(report, loaded.localized_index_report(), "compile and load agree");
    for advice in &report {
        let ddl = &advice.index.as_ref().expect("tv_product is a table").ddl;
        assert!(text.contains(ddl.as_str()), "{text}");
    }

    let (plain, _, _) = emitted("").await;
    let plain: fraiseql_core::schema::CompiledSchema = serde_json::from_value(plain).unwrap();
    assert_eq!(localized_index_report_text(&plain), "", "nothing localized, nothing printed");
}
