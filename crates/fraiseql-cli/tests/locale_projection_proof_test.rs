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
    compile::{CompileOptions, compile_to_schema, emit_ddl_to_dir},
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

/// The compiled schema, the files `--emit-ddl` writes (name → bytes), and the capture-trigger DDL.
async fn emitted(locale: &str) -> (serde_json::Value, Vec<(String, Vec<u8>)>, String) {
    let dir = TempDir::new().unwrap();
    std::fs::write(dir.path().join("schema.json"), SCHEMA_JSON).unwrap();
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
