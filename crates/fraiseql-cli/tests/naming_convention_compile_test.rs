#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable
//! End-to-end coverage for the JSON-schema compile workflow (Workflow-B) naming
//! convention knob (`[fraiseql.naming] convention`).
//!
//! Workflow-B compiles to a `camelCase` GraphQL surface by default (`snake_case`
//! in the database, `camelCase` exposed to clients), overridable to `preserve`. This
//! exercises the real `compile_to_schema` pipeline — including its CWD
//! `fraiseql.toml` lookup — so the compiled schema's `naming_convention` is
//! verified exactly as it would land in `schema.compiled.json`.
//!
//! Single test on purpose: it mutates the process working directory, so keeping
//! it alone in its own test binary avoids racing any sibling test.

use fraiseql_cli::commands::compile::{CompileOptions, compile_to_schema};
use fraiseql_core::schema::NamingConvention;
use tempfile::TempDir;

const SCHEMA_JSON: &str = r#"
{
  "types": [
    {
      "name": "Widget",
      "fields": [
        {"name": "id", "type": "Int", "nullable": false}
      ],
      "sql_source": "v_widgets",
      "is_input": false
    }
  ],
  "queries": [
    {
      "name": "list_widgets",
      "return_type": "Widget",
      "returns_list": true,
      "sql_source": "v_widgets",
      "nullable": false,
      "arguments": []
    }
  ],
  "mutations": [],
  "subscriptions": [],
  "version": "2.0.0"
}
"#;

/// Write `schema.json` (and optionally `fraiseql.toml`) into a fresh temp dir,
/// chdir into it, compile, restore the working directory, and return the
/// compiled schema together with its serialized JSON form.
async fn compile_in_dir(fraiseql_toml: Option<&str>) -> (NamingConvention, String, String) {
    let dir = TempDir::new().expect("temp dir");
    std::fs::write(dir.path().join("schema.json"), SCHEMA_JSON).expect("write schema.json");
    if let Some(toml) = fraiseql_toml {
        std::fs::write(dir.path().join("fraiseql.toml"), toml).expect("write fraiseql.toml");
    }

    let original = std::env::current_dir().expect("cwd");
    std::env::set_current_dir(dir.path()).expect("chdir into temp dir");
    let result = compile_to_schema(CompileOptions::new("schema.json")).await;
    std::env::set_current_dir(original).expect("restore cwd");

    let (schema, _report) = result.expect("compile must succeed");
    let json = serde_json::to_string(&schema).expect("serialize compiled schema");
    (schema.naming_convention, json, schema.display_name("list_widgets"))
}

#[tokio::test]
async fn workflow_b_naming_convention_default_and_override() {
    // No fraiseql.toml at all: Workflow-B defaults to camelCase.
    let (convention, json, display) = compile_in_dir(None).await;
    assert_eq!(convention, NamingConvention::CamelCase, "default must be camelCase");
    assert!(json.contains("\"camelCase\""), "compiled JSON must record camelCase: {json}");
    assert_eq!(display, "listWidgets", "operation names expose camelCase");

    // fraiseql.toml present but no [fraiseql.naming]: still camelCase.
    let toml = "[project]\nname = \"t\"\n\n[fraiseql]\nschema_file = \"schema.json\"\n";
    let (convention, _json, display) = compile_in_dir(Some(toml)).await;
    assert_eq!(convention, NamingConvention::CamelCase, "absent [fraiseql.naming] is camelCase");
    assert_eq!(display, "listWidgets");

    // Explicit opt-out restores the as-authored snake_case surface.
    let toml = "[project]\nname = \"t\"\n\n[fraiseql]\nschema_file = \"schema.json\"\n\n\
                [fraiseql.naming]\nconvention = \"preserve\"\n";
    let (convention, json, display) = compile_in_dir(Some(toml)).await;
    assert_eq!(convention, NamingConvention::Preserve, "preserve override honored");
    assert!(json.contains("\"preserve\""), "compiled JSON must record preserve: {json}");
    assert_eq!(display, "list_widgets", "preserve keeps names as authored");

    // Explicit camelCase is accepted and matches the default.
    let toml = "[project]\nname = \"t\"\n\n[fraiseql]\nschema_file = \"schema.json\"\n\n\
                [fraiseql.naming]\nconvention = \"camelCase\"\n";
    let (convention, _json, display) = compile_in_dir(Some(toml)).await;
    assert_eq!(convention, NamingConvention::CamelCase, "explicit camelCase honored");
    assert_eq!(display, "listWidgets");
}
