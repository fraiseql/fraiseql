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

// ── binary (bit) vectors (#959) ──────────────────────────────────────────────

fn bit_vector_types_json(field_extra: &str) -> String {
    format!(
        r#"{{
  "types": [
    {{
      "name": "Doc",
      "sql_source": "v_doc",
      "fields": [
        {{"name": "id", "type": "ID", "nullable": false}},
        {{"name": "fingerprint", "type": "BitVector", "nullable": false{field_extra}}}
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
fn ir_authored_bit_vector_reaches_the_compiled_schema() {
    let types = bit_vector_types_json(
        r#", "vector_config": {"dimensions": 8, "index_type": "hnsw", "distance_metric": "jaccard"}"#,
    );
    let (ok, log, dir) = run_compile(&types, BASE_TOML, false);
    assert!(ok, "compile must succeed: {log}");

    let compiled = fs::read_to_string(dir.path().join("schema.compiled.json")).unwrap();
    let schema = CompiledSchema::from_json(&compiled, false).expect("compiled schema parses");
    let doc = schema.find_type("Doc").expect("Doc type");
    let field = doc.fields.iter().find(|f| f.name.as_str() == "fingerprint").expect("field");
    assert!(field.is_bit_vector(), "the field type must survive as BitVector, not Vector");
    let config = field.vector_config.as_ref().expect("vector_config must be carried");
    assert_eq!(config.dimensions, 8);
    assert_eq!(config.distance_metric, DistanceMetric::Jaccard);
}

#[test]
fn toml_authored_bit_vector_reaches_the_compiled_schema() {
    let toml = r#"
[types.Doc]
sql_source = "v_doc"

[types.Doc.fields.id]
type = "ID"

[types.Doc.fields.fingerprint]
type = "BitVector"
vector = { dimensions = 768, distance_metric = "hamming" }

[queries.docs]
return_type = "Doc"
return_array = true
sql_source = "v_doc"
"#;
    let (ok, log, dir) = run_compile(r#"{"types": []}"#, toml, false);
    assert!(ok, "TOML-authored BitVector field must compile: {log}");

    let compiled = fs::read_to_string(dir.path().join("schema.compiled.json")).unwrap();
    let schema = CompiledSchema::from_json(&compiled, false).expect("compiled schema parses");
    let doc = schema.find_type("Doc").expect("Doc type");
    let field = doc.fields.iter().find(|f| f.name.as_str() == "fingerprint").expect("field");
    assert!(field.is_bit_vector());
    let config = field.vector_config.as_ref().expect("TOML `vector = {…}` must be carried");
    assert_eq!(config.dimensions, 768);
    assert_eq!(config.distance_metric, DistanceMetric::Hamming);
}

#[test]
fn emitted_bit_vector_ddl_carries_width_bit_ops_index_and_extension() {
    let types = bit_vector_types_json(
        r#", "vector_config": {"dimensions": 8, "index_type": "hnsw", "distance_metric": "hamming"}"#,
    );
    let (ok, log, dir) = run_compile(&types, BASE_TOML, true);
    assert!(ok, "compile with --emit-ddl must succeed: {log}");

    let ddl = fs::read_to_string(dir.path().join("ddl").join("doc.sql")).expect("DDL emitted");
    assert!(
        ddl.contains("fingerprint bit(8)"),
        "the column must carry its width — a bare `bit` column is `bit(1)`: {ddl}"
    );
    assert!(
        ddl.contains("CREATE EXTENSION IF NOT EXISTS vector;"),
        "`bit(N)` is a PostgreSQL type but `<~>` and `bit_hamming_ops` are pgvector's: {ddl}"
    );
    assert!(
        ddl.contains("USING hnsw") && ddl.contains("bit_hamming_ops"),
        "a bit vector indexes with the bit operator class, not vector_*_ops: {ddl}"
    );
}

#[test]
fn a_float_metric_on_a_bit_vector_is_a_compile_error() {
    let types = bit_vector_types_json(
        r#", "vector_config": {"dimensions": 8, "distance_metric": "cosine"}"#,
    );
    let (ok, log, _dir) = run_compile(&types, BASE_TOML, false);
    assert!(!ok, "cosine over a bit(N) column is an operator pgvector does not define");
    assert!(
        log.contains("float vectors") && log.contains("hamming or jaccard"),
        "the error names the metrics a BitVector field takes: {log}"
    );
}

#[test]
fn a_binary_metric_on_a_float_vector_is_a_compile_error() {
    let types =
        vector_types_json(r#", "vector_config": {"dimensions": 3, "distance_metric": "hamming"}"#);
    let (ok, log, _dir) = run_compile(&types, BASE_TOML, false);
    assert!(!ok, "hamming over a vector(N) column is an operator pgvector does not define");
    assert!(
        log.contains("binary (bit) vectors") && log.contains("BitVector"),
        "the error names the field type that carries binary metrics: {log}"
    );
}

/// pgvector 0.8 ships `bit_jaccard_ops` for `hnsw` only, so this combination is
/// DDL `CREATE INDEX` refuses. Caught at compile time, where the author can see
/// it, rather than at migration time against a live database.
#[test]
fn ivfflat_with_jaccard_is_a_compile_error() {
    let types = bit_vector_types_json(
        r#", "vector_config": {"dimensions": 8, "index_type": "ivf_flat", "distance_metric": "jaccard"}"#,
    );
    let (ok, log, _dir) = run_compile(&types, BASE_TOML, false);
    assert!(!ok, "ivfflat has no jaccard operator class for bit vectors");
    assert!(
        log.contains("no ivfflat operator class") && log.contains("hnsw"),
        "the error names the index type that does support it: {log}"
    );
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

// ── the distance in the response (#959) ──────────────────────────────────────

fn distance_types_json(distance_field: &str) -> String {
    format!(
        r#"{{
  "types": [
    {{
      "name": "Doc",
      "sql_source": "v_doc",
      "fields": [
        {{"name": "id", "type": "ID", "nullable": false}},
        {{"name": "embedding", "type": "Vector", "nullable": false,
          "vector_config": {{"dimensions": 3}}}},
        {distance_field}
      ]
    }}
  ]
}}"#
    )
}

#[test]
fn ir_authored_vector_distance_reaches_the_compiled_schema() {
    let types = distance_types_json(
        r#"{"name": "similarity", "type": "Float", "nullable": false,
            "vector_distance": "embedding"}"#,
    );
    let (ok, log, dir) = run_compile(&types, BASE_TOML, false);
    assert!(ok, "compile must succeed: {log}");

    let compiled = fs::read_to_string(dir.path().join("schema.compiled.json")).unwrap();
    let schema = CompiledSchema::from_json(&compiled, false).expect("compiled schema parses");
    let doc = schema.find_type("Doc").expect("Doc type");
    let field = doc.fields.iter().find(|f| f.name.as_str() == "similarity").expect("field");
    assert_eq!(
        field.vector_distance.as_deref(),
        Some("embedding"),
        "the reference must survive IR → converter → compiled schema"
    );
}

#[test]
fn toml_authored_vector_distance_reaches_the_compiled_schema() {
    let toml = r#"
[types.Doc]
sql_source = "v_doc"

[types.Doc.fields.id]
type = "ID"

[types.Doc.fields.embedding]
type = "Vector"
vector = { dimensions = 3 }

[types.Doc.fields.similarity]
type = "Float"
vector_distance = "embedding"

[queries.docs]
return_type = "Doc"
return_array = true
sql_source = "v_doc"
"#;
    let (ok, log, dir) = run_compile(r#"{"types": []}"#, toml, false);
    assert!(ok, "TOML-authored vector_distance must compile: {log}");

    let compiled = fs::read_to_string(dir.path().join("schema.compiled.json")).unwrap();
    let schema = CompiledSchema::from_json(&compiled, false).expect("compiled schema parses");
    let doc = schema.find_type("Doc").expect("Doc type");
    let field = doc.fields.iter().find(|f| f.name.as_str() == "similarity").expect("field");
    assert_eq!(field.vector_distance.as_deref(), Some("embedding"));
}

/// The reference is resolved at compile time. Left to runtime, its only symptom
/// is a query refused on a schema the author already shipped.
#[test]
fn a_vector_distance_naming_no_such_field_is_a_compile_error() {
    let types = distance_types_json(
        r#"{"name": "similarity", "type": "Float", "nullable": false,
            "vector_distance": "embeddings"}"#,
    );
    let (ok, log, _dir) = run_compile(&types, BASE_TOML, false);
    assert!(!ok, "a dangling vector_distance reference must refuse to compile");
    assert!(log.contains("no field by that name"), "the error names the miss: {log}");
}

#[test]
fn a_vector_distance_naming_a_non_vector_field_is_a_compile_error() {
    let types = distance_types_json(
        r#"{"name": "similarity", "type": "Float", "nullable": false,
            "vector_distance": "id"}"#,
    );
    let (ok, log, _dir) = run_compile(&types, BASE_TOML, false);
    assert!(!ok, "a distance to a non-vector field is not a distance");
    assert!(log.contains("is not a vector field"), "got: {log}");
}

#[test]
fn a_non_float_vector_distance_field_is_a_compile_error() {
    let types = distance_types_json(
        r#"{"name": "similarity", "type": "String", "nullable": false,
            "vector_distance": "embedding"}"#,
    );
    let (ok, log, _dir) = run_compile(&types, BASE_TOML, false);
    assert!(!ok, "pgvector's distance operators return double precision");
    assert!(log.contains("a pgvector distance is a Float"), "got: {log}");
}
