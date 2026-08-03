//! #386: the vector authoring surface, end to end through the real CLI binary.
//!
//! Before this, `vector_config` had NO producer: the IR refused the key
//! (`deny_unknown_fields`), the TOML schema had no vector surface, and the
//! converter hardcoded `vector_config: None` — the compiled-schema vector types
//! existed but nothing an operator writes could reach them. These tests pin the
//! whole chain: authored TOML / IR JSON → real `fraiseql-cli compile` →
//! deserialized `CompiledSchema` carrying the config → emitted DDL with a
//! dimensioned column, the declared index, and the extension.
//!
//! **Execution engine:** none · **Infrastructure:** none · **Parallelism:** safe
#![allow(clippy::unwrap_used, clippy::panic)] // Reason: test code, panics acceptable

use std::{fs, process::Command};

use fraiseql_core::schema::{CompiledSchema, DistanceMetric, VectorIndexType};
use tempfile::TempDir;

const BASE_TOML: &str = "";

fn run_compile(types_json: &str, toml_config: &str, emit_ddl: bool) -> (bool, String, TempDir) {
    let temp_dir = TempDir::new().unwrap();
    let types_path = temp_dir.path().join("types.json");
    let toml_path = temp_dir.path().join("fraiseql.toml");
    let output_path = temp_dir.path().join("schema.compiled.json");
    fs::write(&types_path, types_json).unwrap();
    fs::write(&toml_path, toml_config).unwrap();

    let cli_path = env!("CARGO_BIN_EXE_fraiseql-cli");
    let mut args = vec![
        "compile".to_string(),
        toml_path.to_str().unwrap().to_string(),
        "--types".to_string(),
        types_path.to_str().unwrap().to_string(),
        "--output".to_string(),
        output_path.to_str().unwrap().to_string(),
    ];
    if emit_ddl {
        args.push("--emit-ddl".to_string());
        args.push(temp_dir.path().join("ddl").to_str().unwrap().to_string());
    }
    let output = Command::new(cli_path).args(&args).output().expect("failed to run fraiseql-cli");
    let combined = format!(
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    (output.status.success(), combined, temp_dir)
}

fn vector_types_json(field_extra: &str) -> String {
    format!(
        r#"{{
  "types": [
    {{
      "name": "Doc",
      "sql_source": "v_doc",
      "fields": [
        {{"name": "id", "type": "ID", "nullable": false}},
        {{"name": "embedding", "type": "Vector", "nullable": false{field_extra}}}
      ]
    }}
  ],
  "queries": [
    {{
      "name": "docs",
      "return_type": "Doc",
      "returns_list": true,
      "nullable": false,
      "sql_source": "v_doc"
    }}
  ]
}}"#
    )
}

#[test]
fn ir_authored_vector_config_reaches_the_compiled_schema() {
    let types = vector_types_json(
        r#", "vector_config": {"dimensions": 3, "index_type": "hnsw", "distance_metric": "l2"}"#,
    );
    let (ok, log, dir) = run_compile(&types, BASE_TOML, false);
    assert!(ok, "compile must succeed: {log}");

    let compiled = fs::read_to_string(dir.path().join("schema.compiled.json")).unwrap();
    let schema = CompiledSchema::from_json(&compiled, false).expect("compiled schema parses");
    let doc = schema.find_type("Doc").expect("Doc type");
    let field = doc.fields.iter().find(|f| f.name.as_str() == "embedding").expect("field");
    let config = field.vector_config.as_ref().expect(
        "vector_config must survive IR → converter → compiled schema (it used to be \
         hardcoded to None)",
    );
    assert_eq!(config.dimensions, 3);
    assert_eq!(config.index_type, VectorIndexType::Hnsw);
    assert_eq!(config.distance_metric, DistanceMetric::L2);
}

#[test]
fn toml_authored_vector_config_reaches_the_compiled_schema() {
    let toml = r#"
[types.Doc]
sql_source = "v_doc"

[types.Doc.fields.id]
type = "ID"

[types.Doc.fields.embedding]
type = "Vector"
vector = { dimensions = 4, distance_metric = "cosine" }

[queries.docs]
return_type = "Doc"
return_array = true
sql_source = "v_doc"
"#;
    let (ok, log, dir) = run_compile(r#"{"types": []}"#, toml, false);
    assert!(ok, "TOML-authored vector field must compile: {log}");

    let compiled = fs::read_to_string(dir.path().join("schema.compiled.json")).unwrap();
    let schema = CompiledSchema::from_json(&compiled, false).expect("compiled schema parses");
    let doc = schema.find_type("Doc").expect("Doc type");
    let field = doc.fields.iter().find(|f| f.name.as_str() == "embedding").expect("field");
    let config = field.vector_config.as_ref().expect("TOML `vector = {…}` must be carried");
    assert_eq!(config.dimensions, 4);
    assert_eq!(config.distance_metric, DistanceMetric::Cosine);
}

#[test]
fn vector_field_without_config_is_a_compile_error() {
    let types = vector_types_json("");
    let (ok, log, _dir) = run_compile(&types, BASE_TOML, false);
    assert!(!ok, "a Vector field without vector_config must refuse to compile");
    assert!(
        log.contains("vector_config") && log.contains("dimensions"),
        "the error teaches the required shape: {log}"
    );
}

#[test]
fn vector_config_on_a_non_vector_field_is_a_compile_error() {
    let types = r#"{
  "types": [
    {
      "name": "Doc",
      "sql_source": "v_doc",
      "fields": [
        {"name": "title", "type": "String", "nullable": false,
         "vector_config": {"dimensions": 3}}
      ]
    }
  ]
}"#;
    let (ok, log, _dir) = run_compile(types, BASE_TOML, false);
    assert!(!ok, "vector_config on a String field must refuse to compile");
    assert!(log.contains("not Vector"), "the error names the mismatch: {log}");
}

#[test]
fn zero_dimensions_is_a_compile_error() {
    let types = vector_types_json(r#", "vector_config": {"dimensions": 0}"#);
    let (ok, log, _dir) = run_compile(&types, BASE_TOML, false);
    assert!(!ok, "dimensions = 0 must refuse to compile");
    assert!(log.contains("at least 1"), "got: {log}");
}

#[test]
fn emitted_ddl_carries_dimension_index_and_extension() {
    let types = vector_types_json(
        r#", "vector_config": {"dimensions": 3, "index_type": "hnsw", "distance_metric": "cosine"}"#,
    );
    let (ok, log, dir) = run_compile(&types, BASE_TOML, true);
    assert!(ok, "compile with --emit-ddl must succeed: {log}");

    let ddl = fs::read_to_string(dir.path().join("ddl").join("doc.sql")).expect("DDL emitted");
    assert!(
        ddl.contains("embedding vector(3)"),
        "the column must be dimensioned — pgvector cannot index a bare `vector` column: {ddl}"
    );
    assert!(
        ddl.contains("CREATE EXTENSION IF NOT EXISTS vector;"),
        "the extension must accompany vector DDL: {ddl}"
    );
    assert!(
        ddl.contains("USING hnsw") && ddl.contains("vector_cosine_ops"),
        "the declared index type and ops class must be emitted: {ddl}"
    );
}
