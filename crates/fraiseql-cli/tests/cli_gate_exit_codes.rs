#![allow(clippy::unwrap_used, clippy::panic)] // Reason: test code, panics acceptable
//! Integration tests for CLI gate exit codes (Phase 07, honest-failure sweep).
//!
//! Invokes the real CLI binary and asserts that gate-style commands fail the
//! process (non-zero exit) when the checked artifact fails, instead of printing
//! a failure and exiting 0. Covers H22 (`federation check`), H23 (the removed
//! `serve` command) and #1395 (`compile` refusing a federation `@key` the router would
//! reject, on every authoring path). No database required.
//!
//! **Execution engine:** none (CLI binary only)
//! **Infrastructure:** none
//! **Parallelism:** safe

use std::process::Command;

use tempfile::TempDir;

/// A fresh invocation of the compiled CLI binary.
fn cli() -> Command {
    Command::new(env!("CARGO_BIN_EXE_fraiseql-cli"))
}

/// Write `content` to a temp file named `schema.compiled.json` and return its path.
fn write_schema(dir: &TempDir, content: &str) -> String {
    let path = dir.path().join("schema.compiled.json");
    std::fs::write(&path, content).unwrap();
    path.to_string_lossy().into_owned()
}

// ── H22: federation check exit codes ──────────────────────────────

/// An entity keyed on a field its type lacks is a composition error; `federation check`
/// must exit non-zero (gate failure) rather than print the error and exit 0.
#[test]
fn federation_check_composition_error_exits_nonzero() {
    let dir = TempDir::new().unwrap();
    let schema = r#"{
        "types": [ { "name": "User", "sql_source": "v_user",
                     "fields": [ { "name": "id", "field_type": "ID" } ] } ],
        "federation": {
            "enabled": true,
            "version": "v2",
            "entities": [ { "name": "User", "key_fields": ["region"] } ]
        }
    }"#;
    let path = write_schema(&dir, schema);

    let out = cli().args(["federation", "check", &path]).output().unwrap();
    let code = out.status.code().unwrap_or(-1);
    assert_eq!(
        code, 2,
        "federation check with a composition error must exit 2 (validation failure), got {code}"
    );
}

/// A well-formed federated subgraph passes composition and exits 0.
#[test]
fn federation_check_valid_schema_exits_zero() {
    let dir = TempDir::new().unwrap();
    let schema = r#"{
        "types": [ { "name": "User", "sql_source": "v_user",
                     "fields": [ { "name": "id", "field_type": "ID" } ] } ],
        "federation": {
            "enabled": true,
            "version": "v2",
            "entities": [ { "name": "User", "key_fields": ["id"] } ]
        }
    }"#;
    let path = write_schema(&dir, schema);

    let out = cli().args(["federation", "check", &path]).output().unwrap();
    assert!(
        out.status.success(),
        "federation check on a composable subgraph must exit 0, got {:?}",
        out.status
    );
}

// ── H23: the `serve` command is removed (it overwrote its own input) ─

/// `serve` overwrote the source file via a faulty extension swap (`serve
/// fraiseql.toml` derived an identical output path). It is removed; `run
/// --watch` replaces it. The CLI must reject `serve` as an unknown subcommand.
#[test]
fn serve_subcommand_is_removed() {
    let out = cli().args(["serve", "schema.json"]).output().unwrap();
    // clap exits 2 on an unrecognized subcommand.
    assert_eq!(
        out.status.code(),
        Some(2),
        "`serve` must be removed and rejected by the parser, got {:?}",
        out.status
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("unrecognized") || stderr.contains("unexpected"),
        "expected an unknown-subcommand error from clap, got stderr: {stderr}"
    );
}

// ── H23 (defense-in-depth): compile refuses to overwrite its own input ─

/// The deleted `serve` command overwrote the source file by deriving an output
/// path identical to the input. As defense-in-depth against the same class,
/// `compile` must refuse when `--output` equals the input path.
#[test]
fn compile_refuses_to_overwrite_input() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("schema.json");
    std::fs::write(&path, "{}").unwrap();
    let path = path.to_string_lossy().into_owned();

    let out = cli().args(["compile", &path, "--output", &path]).output().unwrap();
    assert!(
        !out.status.success(),
        "compile must refuse to write its output over the input file"
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("Refusing") || stderr.contains("over the input"),
        "expected a refuse-to-overwrite error, got stderr: {stderr}"
    );
}

// ── #1395: compile refuses a federation key the router would reject ─────────

/// `types.json` / `schema.json` shape: `Org` publishes `organizationId` only.
const ORG_TYPES: &str = r#"{
    "types": [ { "name": "Org", "fields": [
        { "name": "id", "type": "ID", "nullable": false },
        { "name": "organizationId", "type": "ID", "nullable": false }
    ] } ],
    "queries": [ { "name": "orgs", "return_type": "Org", "returns_list": true,
                   "sql_source": "v_org" } ]
}"#;

/// Run `compile` with `args` in `dir`; return (exit code, stdout + stderr).
fn compile(dir: &TempDir, args: &[&str]) -> (i32, String) {
    let out = cli().current_dir(dir.path()).arg("compile").args(args).output().unwrap();
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    (out.status.code().unwrap_or(-1), text)
}

/// The SDK path: the issue's repro, an entity keyed on a field its type lacks.
#[test]
fn compile_refuses_an_unknown_key_field_from_schema_json() {
    for (key, accepted) in [
        (r#"["organizationId","region"]"#, false),
        (r#"["organizationId"]"#, true),
    ] {
        let dir = TempDir::new().unwrap();
        let mut doc: serde_json::Value = serde_json::from_str(ORG_TYPES).unwrap();
        doc["federation"] = serde_json::from_str(&format!(
            r#"{{"enabled":true,"version":"v2","entities":[{{"name":"Org","key_fields":{key}}}]}}"#
        ))
        .unwrap();
        std::fs::write(dir.path().join("schema.json"), doc.to_string()).unwrap();

        let (code, text) = compile(&dir, &["schema.json", "-o", "out.json"]);
        if accepted {
            assert_eq!(code, 0, "a valid key must compile: {text}");
        } else {
            assert_ne!(code, 0, "an unknown key field must fail the compile: {text}");
            assert!(text.contains("'Org'") && text.contains("'region'"), "{text}");
        }
    }
}

/// The TOML paths: `[federation]` with types declared in TOML, and with types from
/// `--types`. The second checked no entity at all before #1395: the TOML entity check
/// ran only inside `TomlSchema::validate()`, which the `--types` workflow skips.
#[test]
fn compile_refuses_an_unknown_key_field_from_fraiseql_toml() {
    const TOML_TYPES: &str = r#"
[types.Org]
sql_source = "v_org"
fields.id = { type = "ID" }
fields.organizationId = { type = "ID" }

[queries.orgs]
return_type = "Org"
return_array = true
sql_source = "v_org"
"#;
    for with_types_json in [false, true] {
        for (key, accepted) in [(r#"["region"]"#, false), (r#"["organizationId"]"#, true)] {
            let dir = TempDir::new().unwrap();
            let types = if with_types_json { "" } else { TOML_TYPES };
            let toml = format!(
                "[schema]\nname = \"orgs\"\n{types}\n[federation]\nenabled = true\n\
                 version = \"v2\"\n\n[[federation.entities]]\nname = \"Org\"\n\
                 key_fields = {key}\n"
            );
            std::fs::write(dir.path().join("fraiseql.toml"), toml).unwrap();
            let mut args = vec!["fraiseql.toml", "-o", "out.json"];
            if with_types_json {
                std::fs::write(dir.path().join("types.json"), ORG_TYPES).unwrap();
                args.extend(["--types", "types.json"]);
            }

            let (code, text) = compile(&dir, &args);
            let path = if with_types_json {
                "toml + types.json"
            } else {
                "toml"
            };
            if accepted {
                assert_eq!(code, 0, "{path}: a valid key must compile: {text}");
            } else {
                assert_ne!(code, 0, "{path}: an unknown key field must fail: {text}");
                assert!(text.contains("'Org'") && text.contains("'region'"), "{path}: {text}");
            }
        }
    }
}

/// `federation check` judges the artifact `compile` writes: a valid one passes with no
/// "nothing to check" warning (it used to read a shape compile never writes).
#[test]
fn federation_check_reads_what_compile_writes() {
    let dir = TempDir::new().unwrap();
    let mut doc: serde_json::Value = serde_json::from_str(ORG_TYPES).unwrap();
    doc["federation"] = serde_json::json!({"enabled": true, "version": "v2",
        "entities": [{"name": "Org", "key_fields": ["organizationId"]}]});
    std::fs::write(dir.path().join("schema.json"), doc.to_string()).unwrap();
    let (code, text) = compile(&dir, &["schema.json", "-o", "out.json"]);
    assert_eq!(code, 0, "{text}");

    let out = cli()
        .current_dir(dir.path())
        .args(["federation", "check", "out.json", "--json"])
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(out.status.success(), "{stdout}");
    let result: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(result["data"]["entity_count"], 1, "{stdout}");
    // An empty list is omitted from the result; either way there must be no warning.
    assert!(
        result.get("warnings").is_none_or(|w| w.as_array().is_some_and(Vec::is_empty)),
        "{stdout}"
    );
}
