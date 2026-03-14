#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable
//! Integration tests for `fraiseql lint`.
//!
//! Invokes the real CLI binary via `std::process::Command` and inspects exit
//! codes and stdout/stderr. No database required.
//!
//! **Execution engine:** none (CLI binary only)
//! **Infrastructure:** none
//! **Parallelism:** safe

use std::process::Command;

/// Path to the compiled CLI binary set by Cargo for integration tests.
fn cli() -> Command {
    Command::new(env!("CARGO_BIN_EXE_fraiseql-cli"))
}

/// Absolute path to a fixture file relative to this crate's tests/ directory.
fn fixture(name: &str) -> String {
    format!("{}/tests/fixtures/{name}", env!("CARGO_MANIFEST_DIR"))
}

// ── Basic invocation ──────────────────────────────────────────────

/// `fraiseql lint <schema>` on an empty schema exits 0 and prints a summary.
#[test]
fn lint_on_empty_schema_exits_zero() {
    let out = cli().args(["lint", &fixture("empty_schema.json")]).output().unwrap();
    assert!(out.status.success(), "lint on empty schema must exit 0, got: {:?}", out.status);
}

/// `fraiseql lint <schema>` on a minimal valid schema exits 0.
#[test]
fn lint_on_minimal_schema_exits_zero() {
    let out = cli().args(["lint", &fixture("minimal_schema.json")]).output().unwrap();
    assert!(out.status.success(), "lint on minimal valid schema must exit 0");
}

/// Missing schema file prints an error and exits non-zero.
#[test]
fn lint_on_missing_file_exits_nonzero() {
    let out = cli().args(["lint", "does_not_exist.json"]).output().unwrap();
    assert!(!out.status.success(), "lint on missing file must exit non-zero");
}

// ── JSON output ────────────────────────────────────────────────────

/// `--json` flag produces valid JSON with expected keys.
#[test]
fn lint_json_output_is_valid_json() {
    let out = cli().args(["lint", &fixture("empty_schema.json"), "--json"]).output().unwrap();
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    let parsed: serde_json::Value = serde_json::from_str(&stdout)
        .unwrap_or_else(|e| panic!("lint --json output must be valid JSON: {e}\ngot: {stdout}"));
    assert!(parsed.is_object(), "JSON output must be an object");
}

/// JSON output contains `overall_score` and `severity_counts` fields.
#[test]
fn lint_json_output_contains_expected_fields() {
    let out = cli().args(["lint", &fixture("empty_schema.json"), "--json"]).output().unwrap();
    let stdout = String::from_utf8_lossy(&out.stdout);
    let parsed: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    // data or top-level must contain overall_score
    let data = parsed.get("data").unwrap_or(&parsed);
    assert!(
        data.get("overall_score").is_some(),
        "JSON output must contain `overall_score`, got: {parsed}"
    );
    assert!(
        data.get("severity_counts").is_some(),
        "JSON output must contain `severity_counts`, got: {parsed}"
    );
}

/// An empty schema receives a perfect score (100) when no types are defined.
#[test]
fn lint_empty_schema_scores_100() {
    let out = cli().args(["lint", &fixture("empty_schema.json"), "--json"]).output().unwrap();
    let stdout = String::from_utf8_lossy(&out.stdout);
    let parsed: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    let data = parsed.get("data").unwrap_or(&parsed);
    let score = data["overall_score"].as_u64().unwrap_or(0);
    assert_eq!(score, 100, "empty schema must score 100; got: {parsed}");
}

// ── --fail-on-critical flag ───────────────────────────────────────

/// `--fail-on-critical` on a clean schema exits 0 (no critical issues).
#[test]
fn lint_fail_on_critical_clean_schema_exits_zero() {
    let out = cli()
        .args(["lint", &fixture("empty_schema.json"), "--fail-on-critical"])
        .output()
        .unwrap();
    assert!(out.status.success(), "lint --fail-on-critical on clean schema must exit 0");
}

// ── Category filters ───────────────────────────────────────────────

/// `--federation` filter limits output to federation category only.
#[test]
fn lint_federation_filter_exits_zero() {
    let out = cli()
        .args(["lint", &fixture("minimal_schema.json"), "--federation"])
        .output()
        .unwrap();
    assert!(out.status.success(), "`lint --federation` must exit 0 on valid schema");
}

/// `--cost` filter limits output to cost category only.
#[test]
fn lint_cost_filter_exits_zero() {
    let out = cli()
        .args(["lint", &fixture("minimal_schema.json"), "--cost"])
        .output()
        .unwrap();
    assert!(out.status.success(), "`lint --cost` must exit 0 on valid schema");
}

/// `--cache` filter limits output to cache category only.
#[test]
fn lint_cache_filter_exits_zero() {
    let out = cli()
        .args(["lint", &fixture("minimal_schema.json"), "--cache"])
        .output()
        .unwrap();
    assert!(out.status.success(), "`lint --cache` must exit 0 on valid schema");
}

// ── Exit code contract ────────────────────────────────────────────

/// `lint` exits with code 0 (not 1 or 2) on a valid schema with no flags.
#[test]
fn lint_exit_code_zero_on_success() {
    let out = cli().args(["lint", &fixture("minimal_schema.json")]).output().unwrap();
    let code = out.status.code().unwrap_or(-1);
    assert_eq!(code, 0, "lint on valid schema must exit with code 0, got {code}");
}

/// `lint` on missing file exits with code 1 (error), not 2 (validation failure).
#[test]
fn lint_exit_code_one_on_file_not_found() {
    let out = cli().args(["lint", "missing_file.json"]).output().unwrap();
    let code = out.status.code().unwrap_or(-1);
    assert_eq!(code, 1, "file-not-found must exit with code 1, got {code}");
}
