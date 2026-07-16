#![allow(clippy::unwrap_used, clippy::panic)] // Reason: test code — fail-loud.
//! Integration tests for `fraiseql functions invoke` (phase 08).
//!
//! Each test spawns the compiled CLI binary, which runs the guest in its own V8
//! isolate in a child process — so the harness's one-isolate-per-process constraint
//! is satisfied structurally and these tests are safe under plain `cargo test` (the
//! test process never creates an isolate).
//!
//! **Execution engine:** V8 (in the spawned CLI child)
//! **Infrastructure:** none (no database, no network — host ops are mocked)
//! **Parallelism:** safe (each test forks its own binary + temp dir)

use std::{path::PathBuf, process::Command};

use tempfile::TempDir;

/// The directory holding the fixture `.ts` modules (`module_dir`).
fn fixture_module_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/functions")
}

/// A fresh invocation of the compiled CLI binary.
fn cli() -> Command {
    Command::new(env!("CARGO_BIN_EXE_fraiseql-cli"))
}

/// Write a compiled-schema JSON whose `functions` section declares `notify_approved`
/// (`after:mutation:Order:update`, gated `status` → `approved`) pointing at the
/// fixture module dir. Returns the schema path.
fn write_schema(dir: &TempDir) -> PathBuf {
    let module_dir = fixture_module_dir();
    let schema = serde_json::json!({
        "types": [],
        "queries": [],
        "mutations": [],
        "functions": {
            "module_dir": module_dir,
            "definitions": [
                {
                    "name": "notify_approved",
                    "trigger": "after:mutation:Order:update",
                    "runtime": "Deno",
                    "timeout_ms": null,
                    "when": [ { "field": "status", "changed_to": "approved" } ],
                    "re_runnable": false
                }
            ]
        }
    });
    let path = dir.path().join("schema.compiled.json");
    std::fs::write(&path, serde_json::to_string_pretty(&schema).unwrap()).unwrap();
    path
}

/// Write a JSON fixture file and return its path.
fn write_json(dir: &TempDir, name: &str, value: &serde_json::Value) -> PathBuf {
    let path = dir.path().join(name);
    std::fs::write(&path, serde_json::to_string(value).unwrap()).unwrap();
    path
}

// ── Exit code 0: a matching payload runs, host ops are recorded ──────────────

#[test]
fn matching_payload_runs_and_reports_result_and_host_ops() {
    let dir = TempDir::new().unwrap();
    let schema = write_schema(&dir);
    // status pending → approved satisfies the `changed_to` predicate.
    let payload = write_json(
        &dir,
        "payload.json",
        &serde_json::json!({
            "event_kind": "update",
            "old": { "id": "o-1", "status": "pending" },
            "new": { "id": "o-1", "status": "approved" }
        }),
    );
    // A query mock matching any query (no `query_contains`) returns a canned row.
    let mock_query = write_json(
        &dir,
        "query.json",
        &serde_json::json!([ { "response": { "data": { "markNotified": { "id": "o-1" } } } } ]),
    );

    let out = cli()
        .args(["functions", "invoke", "notify_approved"])
        .arg("--payload")
        .arg(&payload)
        .arg("--schema")
        .arg(&schema)
        .arg("--mock-query")
        .arg(&mock_query)
        .args(["--idempotency-token", "tok-123"])
        .output()
        .unwrap();

    let stdout = String::from_utf8_lossy(&out.stdout);
    let code = out.status.code().unwrap_or(-1);
    assert_eq!(code, 0, "a matching payload runs (exit 0); stdout:\n{stdout}");
    assert!(stdout.contains("notified"), "the guest's result is printed:\n{stdout}");
    // The injected idempotency token reached the guest and came back in the result.
    assert!(stdout.contains("tok-123"), "the injected token is visible:\n{stdout}");
    // The recorded host-op calls are printed (the query the guest issued).
    assert!(stdout.contains("host ops"), "host-op calls are reported:\n{stdout}");
    assert!(stdout.contains("query"), "the guest's query op is recorded:\n{stdout}");
}

// ── Exit code 3: the `when` predicate does not match — no isolate spins ──────

#[test]
fn non_matching_predicate_exits_predicate_no_match() {
    let dir = TempDir::new().unwrap();
    let schema = write_schema(&dir);
    // Already approved → the `changed_to approved` transition did NOT happen.
    let payload = write_json(
        &dir,
        "payload.json",
        &serde_json::json!({
            "event_kind": "update",
            "old": { "id": "o-2", "status": "approved" },
            "new": { "id": "o-2", "status": "approved" }
        }),
    );

    let out = cli()
        .args(["functions", "invoke", "notify_approved"])
        .arg("--payload")
        .arg(&payload)
        .arg("--schema")
        .arg(&schema)
        .arg("--explain")
        .output()
        .unwrap();

    let stdout = String::from_utf8_lossy(&out.stdout);
    let code = out.status.code().unwrap_or(-1);
    assert_eq!(code, 3, "a non-matching predicate exits 3 (no isolate); stdout:\n{stdout}");
    assert!(stdout.contains("NO MATCH"), "--explain shows why it did not fire:\n{stdout}");
}

// ── Exit code 4: a mock miss fails the op loud → the guest errors ────────────

#[test]
fn mock_query_miss_fails_loud_and_the_guest_errors() {
    let dir = TempDir::new().unwrap();
    let schema = write_schema(&dir);
    let payload = write_json(
        &dir,
        "payload.json",
        &serde_json::json!({
            "event_kind": "update",
            "old": { "id": "o-3", "status": "pending" },
            "new": { "id": "o-3", "status": "approved" }
        }),
    );
    // A mock that matches a DIFFERENT query — the guest's query matches none.
    let mock_query = write_json(
        &dir,
        "query.json",
        &serde_json::json!([ { "query_contains": "somethingElse", "response": {} } ]),
    );

    let out = cli()
        .args(["functions", "invoke", "notify_approved"])
        .arg("--payload")
        .arg(&payload)
        .arg("--schema")
        .arg(&schema)
        .arg("--mock-query")
        .arg(&mock_query)
        .output()
        .unwrap();

    let code = out.status.code().unwrap_or(-1);
    assert_eq!(code, 4, "an unmatched mock fails the op loud → guest error (exit 4)");
}

// ── Exit code 1: an unknown function is a config error ───────────────────────

#[test]
fn unknown_function_is_a_config_error() {
    let dir = TempDir::new().unwrap();
    let schema = write_schema(&dir);
    let payload = write_json(&dir, "payload.json", &serde_json::json!({ "id": "x" }));

    let out = cli()
        .args(["functions", "invoke", "does_not_exist"])
        .arg("--payload")
        .arg(&payload)
        .arg("--schema")
        .arg(&schema)
        .output()
        .unwrap();

    let code = out.status.code().unwrap_or(-1);
    assert_eq!(code, 1, "an unknown function name is a config error (exit 1)");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("does_not_exist") || stderr.to_lowercase().contains("no function"),
        "the error names the missing function:\n{stderr}"
    );
}
