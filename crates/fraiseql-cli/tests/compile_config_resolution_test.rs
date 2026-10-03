#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable
//! #1387 — a schema compiles with the `fraiseql.toml` of its own project, whatever
//! the working directory, and the compile says which file it read.
//!
//! Drives the real binary in the issue's layout: two subgraphs in one repository,
//! each with its own config, the compile run from the *other* subgraph's
//! directory. The configs differ in `[fraiseql.naming] convention`, which lands in
//! the artifact as `naming_convention`, so the test reads the effect, not only the
//! announcement.
//!
//! **Execution engine:** none · **Infrastructure:** none · **Parallelism:** safe

use std::{fs, path::Path, process::Command};

use serde_json::Value;
use tempfile::TempDir;

const SCHEMA_JSON: &str = r#"{
  "types": [{"name": "Widget", "sql_source": "v_widget", "is_input": false,
             "fields": [{"name": "id", "type": "Int", "nullable": false}]}],
  "queries": [{"name": "list_widgets", "return_type": "Widget", "returns_list": true,
               "sql_source": "v_widget", "nullable": false, "arguments": []}],
  "mutations": []
}"#;

const PRESERVE: &str = "[fraiseql.naming]\nconvention = \"preserve\"\n";
const CAMEL: &str = "[fraiseql.naming]\nconvention = \"camelCase\"\n";

/// `repo/.git`, `repo/alpha/fraiseql.toml` (preserve) with `alpha/schema/schema.json`,
/// and `repo/beta/fraiseql.toml` (camelCase).
fn two_subgraphs() -> TempDir {
    let repo = TempDir::new().unwrap();
    fs::create_dir(repo.path().join(".git")).unwrap();
    fs::create_dir_all(repo.path().join("alpha/schema")).unwrap();
    fs::create_dir_all(repo.path().join("beta")).unwrap();
    fs::write(repo.path().join("alpha/fraiseql.toml"), PRESERVE).unwrap();
    fs::write(repo.path().join("alpha/schema/schema.json"), SCHEMA_JSON).unwrap();
    fs::write(repo.path().join("beta/fraiseql.toml"), CAMEL).unwrap();
    repo
}

/// Compile `schema` from `cwd` with `extra` arguments; (stdout, artifact).
fn compile(cwd: &Path, schema: &Path, out: &Path, extra: &[&str]) -> (String, Value) {
    let output = Command::new(env!("CARGO_BIN_EXE_fraiseql-cli"))
        .current_dir(cwd)
        .arg("compile")
        .arg(schema)
        .arg("-o")
        .arg(out)
        .args(extra)
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    assert!(
        output.status.success(),
        "compile failed\nstdout:\n{stdout}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    (stdout, serde_json::from_str(&fs::read_to_string(out).unwrap()).unwrap())
}

#[test]
fn compiling_from_a_sibling_subgraph_uses_the_schemas_own_config() {
    let repo = two_subgraphs();
    let out = repo.path().join("out.json");
    let schema = repo.path().join("alpha/schema/schema.json");

    let (stdout, artifact) = compile(&repo.path().join("beta"), &schema, &out, &[]);

    assert_eq!(
        artifact["naming_convention"], "preserve",
        "alpha's schema must compile with alpha's config, not the working directory's"
    );
    let alpha_config = repo.path().join("alpha/fraiseql.toml");
    assert!(
        stdout.contains(&format!("Config: {}", alpha_config.display())),
        "the compile must name the config it read; stdout:\n{stdout}"
    );
}

#[test]
fn an_explicit_config_overrides_discovery() {
    let repo = two_subgraphs();
    let out = repo.path().join("out.json");
    let schema = repo.path().join("alpha/schema/schema.json");
    let beta_config = repo.path().join("beta/fraiseql.toml");

    let (stdout, artifact) =
        compile(repo.path(), &schema, &out, &["--config", beta_config.to_str().unwrap()]);

    assert_eq!(artifact["naming_convention"], "camelCase");
    assert!(stdout.contains("(--config)"), "stdout:\n{stdout}");
}
