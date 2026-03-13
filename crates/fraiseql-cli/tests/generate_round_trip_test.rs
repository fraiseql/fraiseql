#![allow(clippy::unwrap_used)]  // Reason: test/bench code, panics are acceptable
//! Round-trip integration tests for `fraiseql generate` → `fraiseql extract`.
//!
//! For each language: generate source from a known schema.json, then extract
//! it back, and verify the schema matches the original.

use std::fs;

use tempfile::TempDir;

/// Canonical test schema in JSON form.
const fn canonical_schema_json() -> &'static str {
    r#"{
  "version": "2.0.0",
  "types": [
    {
      "name": "Author",
      "fields": [
        { "name": "pk", "type": "Int", "nullable": false },
        { "name": "id", "type": "ID", "nullable": false },
        { "name": "name", "type": "String", "nullable": false },
        { "name": "bio", "type": "String", "nullable": true }
      ]
    }
  ],
  "queries": [
    {
      "name": "authors",
      "return_type": "Author",
      "returns_list": true,
      "nullable": false,
      "arguments": [],
      "sql_source": "v_author"
    },
    {
      "name": "author",
      "return_type": "Author",
      "returns_list": false,
      "nullable": false,
      "arguments": [
        { "name": "id", "type": "ID", "nullable": false }
      ],
      "sql_source": "v_author"
    }
  ],
  "enums": [],
  "mutations": []
}"#
}

/// Helper: write canonical schema to a temp file.
fn write_schema(dir: &TempDir) -> String {
    let path = dir.path().join("schema.json");
    fs::write(&path, canonical_schema_json()).unwrap();
    path.to_str().unwrap().to_string()
}

/// Run the generate command programmatically.
fn run_generate(schema_path: &str, language: &str, output_path: &str) {
    let lang = language.parse::<fraiseql_cli::commands::init::Language>().unwrap();
    fraiseql_cli::commands::generate::run(schema_path, lang, Some(output_path)).unwrap();
}

/// Run the extract command and return the resulting schema JSON.
fn run_extract(source_path: &str, language: &str, output_path: &str) {
    fraiseql_cli::commands::extract::run(
        &[source_path.to_string()],
        Some(language),
        false,
        output_path,
    )
    .unwrap();
}

/// Parse the extracted schema and check it matches the canonical one.
fn verify_round_trip(extracted_json_path: &str) {
    let json = fs::read_to_string(extracted_json_path).unwrap();
    let schema: serde_json::Value = serde_json::from_str(&json).unwrap();

    let types = schema["types"].as_array().unwrap();
    assert_eq!(types.len(), 1, "Expected 1 type");
    assert_eq!(types[0]["name"], "Author");

    let fields = types[0]["fields"].as_array().unwrap();
    assert_eq!(fields.len(), 4, "Expected 4 fields");
    assert_eq!(fields[0]["name"], "pk");
    assert_eq!(fields[0]["type"], "Int");
    assert!(!fields[0]["nullable"].as_bool().unwrap());
    assert_eq!(fields[1]["name"], "id");
    assert_eq!(fields[1]["type"], "ID");
    assert_eq!(fields[2]["name"], "name");
    assert_eq!(fields[2]["type"], "String");
    assert_eq!(fields[3]["name"], "bio");
    assert_eq!(fields[3]["type"], "String");
    assert!(fields[3]["nullable"].as_bool().unwrap());

    let queries = schema["queries"].as_array().unwrap();
    assert_eq!(queries.len(), 2, "Expected 2 queries");
    assert_eq!(queries[0]["name"], "authors");
    assert!(queries[0]["returns_list"].as_bool().unwrap());
    assert_eq!(queries[0]["return_type"], "Author");
    assert_eq!(queries[1]["name"], "author");
    assert!(!queries[1]["returns_list"].as_bool().unwrap());

    let args = queries[1]["arguments"].as_array().unwrap();
    assert_eq!(args.len(), 1, "Expected 1 argument on 'author' query");
    assert_eq!(args[0]["name"], "id");
    assert_eq!(args[0]["type"], "ID");
}

macro_rules! round_trip_test {
    ($test_name:ident, $lang:expr, $ext:expr) => {
        #[test]
        fn $test_name() {
            let dir = TempDir::new().unwrap();
            let schema_path = write_schema(&dir);

            let source_path =
                dir.path().join(format!("schema.{}", $ext)).to_str().unwrap().to_string();
            let extracted_path = dir.path().join("extracted.json").to_str().unwrap().to_string();

            run_generate(&schema_path, $lang, &source_path);
            assert!(
                std::path::Path::new(&source_path).exists(),
                "Generated source file should exist"
            );

            run_extract(&source_path, $lang, &extracted_path);
            verify_round_trip(&extracted_path);
        }
    };
}

round_trip_test!(test_round_trip_python, "python", "py");
round_trip_test!(test_round_trip_typescript, "typescript", "ts");
round_trip_test!(test_round_trip_rust, "rust", "rs");
round_trip_test!(test_round_trip_kotlin, "kotlin", "kt");
round_trip_test!(test_round_trip_swift, "swift", "swift");
round_trip_test!(test_round_trip_scala, "scala", "scala");
round_trip_test!(test_round_trip_java, "java", "java");
round_trip_test!(test_round_trip_go, "go", "go");
round_trip_test!(test_round_trip_csharp, "csharp", "cs");
