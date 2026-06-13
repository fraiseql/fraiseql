#![allow(clippy::unwrap_used, clippy::panic)] // Reason: test code, panics acceptable
//! Integration tests for CLI gate exit codes (Phase 07, honest-failure sweep).
//!
//! Invokes the real CLI binary and asserts that gate-style commands fail the
//! process (non-zero exit) when the checked artifact fails, instead of printing
//! a failure and exiting 0. Covers H22 (`federation check`) and H23 (the removed
//! `serve` command). No database required.
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

/// A federated type with no `@key` is a composition error; `federation check`
/// must exit non-zero (gate failure) rather than print the error and exit 0.
#[test]
fn federation_check_composition_error_exits_nonzero() {
    let dir = TempDir::new().unwrap();
    // `keys: []` triggers "Type 'User' has no @key directive" → validation-failed.
    let schema = r#"{
        "federation": {
            "enabled": true,
            "version": "v2",
            "types": [ { "name": "User", "keys": [] } ]
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
        "federation": {
            "enabled": true,
            "version": "v2",
            "types": [ { "name": "User", "keys": [ { "fields": ["id"] } ] } ]
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
