#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable
//! Integration tests for `fraiseql analyze`.
//!
//! Invokes the real CLI binary and inspects exit codes and JSON output.
//! No database required — `analyze` works purely on the schema file.
//!
//! **Execution engine:** none (CLI binary only)
//! **Infrastructure:** none (filesystem only)
//! **Parallelism:** safe

use std::process::Command;

fn cli() -> Command {
    Command::new(env!("CARGO_BIN_EXE_fraiseql-cli"))
}

fn fixture(name: &str) -> String {
    format!("{}/tests/fixtures/{name}", env!("CARGO_MANIFEST_DIR"))
}

// ── Basic invocation ──────────────────────────────────────────────────────────

/// `fraiseql analyze <schema>` on a valid schema exits 0.
#[test]
fn analyze_on_valid_schema_exits_zero() {
    let out = cli()
        .args(["analyze", &fixture("empty_schema.json")])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "analyze on valid schema must exit 0; stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

/// Missing schema file exits non-zero.
#[test]
fn analyze_on_missing_file_exits_nonzero() {
    let out = cli()
        .args(["analyze", "does_not_exist.json"])
        .output()
        .unwrap();
    assert!(!out.status.success(), "analyze on missing file must exit non-zero");
}

/// An invalid JSON file exits non-zero.
#[test]
fn analyze_on_invalid_json_exits_nonzero() {
    let dir = tempfile::tempdir().unwrap();
    let bad_file = dir.path().join("bad.json");
    std::fs::write(&bad_file, b"not valid json at all {{{").unwrap();
    let out = cli()
        .args(["analyze", bad_file.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(!out.status.success(), "analyze on invalid JSON must exit non-zero");
}

// ── JSON output ───────────────────────────────────────────────────────────────

/// `--json` flag produces valid JSON output.
#[test]
fn analyze_json_output_is_valid_json() {
    let out = cli()
        .args(["analyze", &fixture("empty_schema.json"), "--json"])
        .output()
        .unwrap();
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    let parsed: serde_json::Value = serde_json::from_str(&stdout)
        .unwrap_or_else(|e| panic!("analyze --json output must be valid JSON: {e}\ngot: {stdout}"));
    assert!(parsed.is_object(), "JSON output must be an object");
}

/// JSON output contains a `categories` field with analysis sections.
#[test]
fn analyze_json_output_contains_categories() {
    let out = cli()
        .args(["analyze", &fixture("empty_schema.json"), "--json"])
        .output()
        .unwrap();
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    let parsed: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    // data wrapper or top-level
    let data = parsed.get("data").unwrap_or(&parsed);
    assert!(
        data.get("categories").is_some(),
        "JSON output must contain `categories`; got: {parsed}"
    );
}

/// JSON output `categories` contains at least the 6 expected sections.
#[test]
fn analyze_json_output_has_six_categories() {
    let out = cli()
        .args(["analyze", &fixture("empty_schema.json"), "--json"])
        .output()
        .unwrap();
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    let parsed: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    let data = parsed.get("data").unwrap_or(&parsed);
    let cats = data["categories"].as_object().expect("categories must be object");
    assert!(
        cats.len() >= 6,
        "analyze must return at least 6 categories, got {}; keys: {:?}",
        cats.len(),
        cats.keys().collect::<Vec<_>>()
    );
}

/// JSON output `summary` contains a `health_score` field.
#[test]
fn analyze_json_output_has_health_score() {
    let out = cli()
        .args(["analyze", &fixture("empty_schema.json"), "--json"])
        .output()
        .unwrap();
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    let parsed: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    let data = parsed.get("data").unwrap_or(&parsed);
    let summary = data.get("summary").expect("JSON output must contain `summary`");
    assert!(
        summary.get("health_score").is_some(),
        "summary must contain `health_score`; got: {summary}"
    );
}

// ── Category presence ─────────────────────────────────────────────────────────

/// Output mentions expected categories: performance, security, federation.
#[test]
fn analyze_output_mentions_key_categories() {
    let out = cli()
        .args(["analyze", &fixture("empty_schema.json"), "--json"])
        .output()
        .unwrap();
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    for cat in &["performance", "security", "federation"] {
        assert!(
            stdout.contains(cat),
            "analyze output must mention category `{cat}`; output: {stdout}"
        );
    }
}

/// Each category in JSON output has at least one recommendation.
#[test]
fn analyze_each_category_has_recommendations() {
    let out = cli()
        .args(["analyze", &fixture("empty_schema.json"), "--json"])
        .output()
        .unwrap();
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    let parsed: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    let data = parsed.get("data").unwrap_or(&parsed);
    let cats = data["categories"].as_object().expect("categories must be object");
    for (name, recs) in cats {
        let arr = recs.as_array().unwrap_or_else(|| panic!("category {name} must be an array"));
        assert!(!arr.is_empty(), "category {name} must have at least one recommendation");
    }
}

// ── Exit code contract ────────────────────────────────────────────────────────

/// `analyze` exits with code 0 on a valid schema.
#[test]
fn analyze_exit_code_zero_on_success() {
    let out = cli()
        .args(["analyze", &fixture("minimal_schema.json")])
        .output()
        .unwrap();
    let code = out.status.code().unwrap_or(-1);
    assert_eq!(code, 0, "analyze on valid schema must exit with code 0, got {code}");
}

/// `analyze` exits with code 1 (error) on missing file.
#[test]
fn analyze_exit_code_one_on_file_not_found() {
    let out = cli().args(["analyze", "missing_file.json"]).output().unwrap();
    let code = out.status.code().unwrap_or(-1);
    assert_eq!(code, 1, "file-not-found must exit with code 1, got {code}");
}
