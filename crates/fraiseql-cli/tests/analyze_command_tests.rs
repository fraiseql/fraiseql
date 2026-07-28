#![allow(clippy::unwrap_used, clippy::panic)] // Reason: test code, panics acceptable
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
    let out = cli().args(["analyze", &fixture("empty_schema.json")]).output().unwrap();
    assert!(
        out.status.success(),
        "analyze on valid schema must exit 0; stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

/// Missing schema file exits non-zero.
#[test]
fn analyze_on_missing_file_exits_nonzero() {
    let out = cli().args(["analyze", "does_not_exist.json"]).output().unwrap();
    assert!(!out.status.success(), "analyze on missing file must exit non-zero");
}

/// An invalid JSON file exits non-zero.
#[test]
fn analyze_on_invalid_json_exits_nonzero() {
    let dir = tempfile::tempdir().unwrap();
    let bad_file = dir.path().join("bad.json");
    std::fs::write(&bad_file, b"not valid json at all {{{").unwrap();
    let out = cli().args(["analyze", bad_file.to_str().unwrap()]).output().unwrap();
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

/// JSON output contains the `recommendations` array the published contract describes.
#[test]
fn analyze_json_output_contains_recommendations() {
    let out = cli()
        .args(["analyze", &fixture("empty_schema.json"), "--json"])
        .output()
        .unwrap();
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    let parsed: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    // data wrapper or top-level
    let data = parsed.get("data").unwrap_or(&parsed);
    // `categories` was a map of constant strings; `recommendations` is the shape
    // `--show-output-schema analyze` has always published (#818).
    assert!(
        data.get("recommendations").and_then(serde_json::Value::as_array).is_some(),
        "JSON output must contain a `recommendations` array; got: {parsed}"
    );
}

/// Every recommendation carries the four fields the published contract declares.
#[test]
fn analyze_recommendations_match_the_published_contract() {
    let out = cli()
        .args(["analyze", &fixture("empty_schema.json"), "--json"])
        .output()
        .unwrap();
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    let parsed: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    let data = parsed.get("data").unwrap_or(&parsed);
    let recs = data["recommendations"].as_array().expect("recommendations must be an array");
    assert!(!recs.is_empty(), "an analysed schema must produce findings");
    for rec in recs {
        for key in ["category", "severity", "message", "suggestion"] {
            assert!(rec.get(key).is_some(), "recommendation missing `{key}`: {rec}");
        }
    }
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

/// A schema that configures no security controls is reported as such, and does not
/// score full marks.
#[test]
fn analyze_reports_an_unconfigured_schema_honestly() {
    // `empty_schema.json` is literally `{}`. It used to produce the same 100/100 report
    // as every other input, affirming that rate limiting and audit logging were active
    // (#818).
    let out = cli()
        .args(["analyze", &fixture("empty_schema.json"), "--json"])
        .output()
        .unwrap();
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    let parsed: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    let data = parsed.get("data").unwrap_or(&parsed);

    assert!(
        stdout.contains("security"),
        "the report must still cover security; output: {stdout}"
    );
    assert!(
        !stdout.contains("Rate limiting configured and active"),
        "an empty schema must not be told its rate limiting is active; output: {stdout}"
    );
    let score = data["summary"]["health_score"].as_u64().expect("health_score");
    assert!(score < 100, "an empty schema cannot be in perfect health, got {score}");
}

/// Two schemas with different contents produce different reports.
#[test]
fn analyze_output_depends_on_the_schema_it_is_given() {
    // The single most important property, and the one the old implementation failed:
    // `{}` and a real schema produced byte-identical output (#818).
    let empty = cli()
        .args(["analyze", &fixture("empty_schema.json"), "--json"])
        .output()
        .unwrap();
    let minimal = cli()
        .args([
            "analyze",
            &fixture("analyze_compiled_schema.json"),
            "--json",
        ])
        .output()
        .unwrap();
    assert!(empty.status.success() && minimal.status.success());

    let strip = |out: &std::process::Output| {
        let text = String::from_utf8_lossy(&out.stdout).to_string();
        let mut v: serde_json::Value = serde_json::from_str(&text).unwrap();
        // The echoed path differs by construction; remove it so the comparison is
        // about the analysis.
        if let Some(data) = v.get_mut("data").and_then(serde_json::Value::as_object_mut) {
            data.remove("schema_file");
        }
        v
    };

    assert_ne!(
        strip(&empty),
        strip(&minimal),
        "analyze must report on the schema it was given, not a constant"
    );
}

// ── Exit code contract ────────────────────────────────────────────────────────

/// `analyze` exits with code 0 on a real compiled schema.
///
/// Deliberately not `minimal_schema.json`: that fixture is a hand-written
/// approximation (`type` where `FieldDefinition` declares `field_type`) which never
/// deserialized into a `CompiledSchema`. `analyze` used to accept it because it parsed
/// into an untyped `serde_json::Value` and then discarded it (#818).
/// `analyze_compiled_schema.json` is emitted from the real types — see the
/// `emit_integration_fixture` test in the crate.
#[test]
fn analyze_exit_code_zero_on_success() {
    let out = cli()
        .args(["analyze", &fixture("analyze_compiled_schema.json")])
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
