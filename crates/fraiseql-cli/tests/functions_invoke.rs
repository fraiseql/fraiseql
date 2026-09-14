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

/// Author a project declaring `notify_approved`
/// (`after:mutation:Order:update`, gated `status` → `approved`), **compile it with the
/// real compiler**, and return the path to the compiled artifact.
///
/// This used to hand-build the compiled JSON, because until #1325 nothing could produce
/// one — `IntermediateSchema` had no `functions` field, so the only writer of a compiled
/// `functions` section in the tree was this function. A harness fixture that constructs
/// its own input proves only that it agrees with itself; every question about whether
/// the compiler and the harness read the same section was invisible to it. Now the
/// producer is the producer.
fn write_schema(dir: &TempDir) -> PathBuf {
    let module_dir = fixture_module_dir();
    let types = serde_json::json!({
        "version": "2.0.0",
        "types": [{
            "name": "Order",
            "sql_source": "v_order",
            "is_input": false,
            "fields": [
                {"name": "id", "type": "ID", "nullable": false},
                {"name": "status", "type": "String", "nullable": false}
            ]
        }],
        "mutations": [{
            "name": "updateOrder",
            "return_type": "Order",
            "sql_source": "fn_update_order",
            "operation": "update",
            "invalidates_views": ["v_order"],
            "arguments": []
        }],
        "functions": [{
            "name": "notify_approved",
            "trigger": "after:mutation:Order:update",
            "runtime": "Deno",
            "when": [{"field": "status", "changed_to": "approved"}]
        }]
    });
    let types_path = dir.path().join("types.json");
    std::fs::write(&types_path, serde_json::to_string_pretty(&types).unwrap()).unwrap();

    // `module_dir` is the deployment half of the surface, owned by `[functions]` — it
    // is not something the schema can set, which is what the split is for.
    let toml_path = dir.path().join("fraiseql.toml");
    std::fs::write(
        &toml_path,
        format!(
            "[schema]\nname = \"invoke\"\nversion = \"1.0.0\"\n\
             database_target = \"postgresql\"\n\n\
             [functions]\nmodule_dir = \"{}\"\n",
            module_dir.display()
        ),
    )
    .unwrap();

    let path = dir.path().join("schema.compiled.json");
    let output = cli()
        .arg("compile")
        .arg(&toml_path)
        .arg("--types")
        .arg(&types_path)
        .arg("--output")
        .arg(&path)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "the fixture project must compile:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
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

// ── before:mutation — a data-dependent rule (#1328) ──────────────────────────

/// Author a project declaring `credit_limit` (`before:mutation:placeOrder`), compile
/// it with the real compiler, and return the compiled artifact's path.
fn write_before_mutation_schema(dir: &TempDir) -> PathBuf {
    let module_dir = fixture_module_dir();
    let types = serde_json::json!({
        "version": "2.0.0",
        "types": [{
            "name": "Order",
            "sql_source": "v_order",
            "is_input": false,
            "fields": [
                {"name": "id", "type": "ID", "nullable": false},
                {"name": "amount", "type": "Int", "nullable": false}
            ]
        }],
        "mutations": [{
            "name": "placeOrder",
            "return_type": "Order",
            "sql_source": "fn_place_order",
            "operation": "insert",
            "invalidates_views": ["v_order"],
            "arguments": []
        }],
        "functions": [{
            "name": "credit_limit",
            "trigger": "before:mutation:placeOrder",
            "runtime": "Deno"
        }]
    });
    let types_path = dir.path().join("types.json");
    std::fs::write(&types_path, serde_json::to_string_pretty(&types).unwrap()).unwrap();

    let toml_path = dir.path().join("fraiseql.toml");
    std::fs::write(
        &toml_path,
        format!(
            "[schema]\nname = \"invoke\"\nversion = \"1.0.0\"\n\
             database_target = \"postgresql\"\n\n\
             [functions]\nmodule_dir = \"{}\"\n",
            module_dir.display()
        ),
    )
    .unwrap();

    let path = dir.path().join("schema.compiled.json");
    let output = cli()
        .arg("compile")
        .arg(&toml_path)
        .arg("--types")
        .arg(&types_path)
        .arg("--output")
        .arg(&path)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "the fixture project must compile:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    path
}

/// The read is mocked with a customer whose remaining credit is `limit - outstanding`.
fn credit_mock(dir: &TempDir, limit: i64, outstanding: i64) -> PathBuf {
    write_json(
        dir,
        "query.json",
        &serde_json::json!([{
            "query_contains": "customer",
            "response": { "data": { "customer": {
                "creditLimit": limit, "outstanding": outstanding,
            } } }
        }]),
    )
}

/// A rule that *needs* a read runs in the harness, and the harness reports the
/// decision the server would reach — not just the guest's raw JSON.
///
/// This is the fixture #1328 asks for: without the read bridge the guest's
/// `fraiseql_query` fails loud and this function cannot decide anything.
#[test]
fn a_data_dependent_rule_aborts_when_the_read_says_so() {
    let dir = TempDir::new().unwrap();
    let schema = write_before_mutation_schema(&dir);
    // The fixture is the mutation's resolved arguments — there are no row images.
    let payload = write_json(
        &dir,
        "payload.json",
        &serde_json::json!({ "input": { "customer_id": "c-1", "amount": 900 } }),
    );
    let mock_query = credit_mock(&dir, 1000, 400); // 600 remaining < 900

    let out = cli()
        .args(["functions", "invoke", "credit_limit"])
        .arg("--payload")
        .arg(&payload)
        .arg("--schema")
        .arg(&schema)
        .arg("--mock-query")
        .arg(&mock_query)
        .output()
        .unwrap();

    let stdout = String::from_utf8_lossy(&out.stdout);
    assert_eq!(out.status.code().unwrap_or(-1), 0, "the guest ran; stdout:\n{stdout}");
    assert!(
        stdout.contains("decision: ABORT `placeOrder`"),
        "the harness must report the decision the chain would reach:\n{stdout}"
    );
    assert!(
        stdout.contains("exceeds the remaining credit of 600"),
        "the rule's own message must be shown:\n{stdout}"
    );
}

/// The other side of the same rule: the counterweight that proves the abort above
/// is a verdict about the data and not a fixture that refuses everything. It also
/// pins the rewrite path — a hook that returns `{input}` replaces the arguments the
/// write binds from.
#[test]
fn the_same_rule_proceeds_and_rewrites_when_the_read_allows_it() {
    let dir = TempDir::new().unwrap();
    let schema = write_before_mutation_schema(&dir);
    let payload = write_json(
        &dir,
        "payload.json",
        &serde_json::json!({ "input": { "customer_id": "c-1", "amount": 100 } }),
    );
    let mock_query = credit_mock(&dir, 1000, 400); // 600 remaining > 100

    let out = cli()
        .args(["functions", "invoke", "credit_limit"])
        .arg("--payload")
        .arg(&payload)
        .arg("--schema")
        .arg(&schema)
        .arg("--mock-query")
        .arg(&mock_query)
        .arg("--json")
        .output()
        .unwrap();

    let stdout = String::from_utf8_lossy(&out.stdout);
    assert_eq!(out.status.code().unwrap_or(-1), 0, "the guest ran; stdout:\n{stdout}");
    let decision: serde_json::Value = stdout
        .lines()
        .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
        .find(|value| value.get("decision").is_some())
        .expect("--json must emit the decision object");
    assert_eq!(decision["decision"], "proceed");
    assert_eq!(decision["rewritten"], true, "the hook stamped the input: {decision}");
    assert_eq!(decision["arguments_or_reason"]["credit_checked"], true);
}

/// A `before:mutation` fixture is the arguments object. A bare array or scalar is a
/// config error naming what the shape should be, not a payload the guest is handed.
#[test]
fn a_non_object_before_mutation_fixture_is_a_config_error() {
    let dir = TempDir::new().unwrap();
    let schema = write_before_mutation_schema(&dir);
    let payload = write_json(&dir, "payload.json", &serde_json::json!([1, 2, 3]));

    let out = cli()
        .args(["functions", "invoke", "credit_limit"])
        .arg("--payload")
        .arg(&payload)
        .arg("--schema")
        .arg(&schema)
        .output()
        .unwrap();

    assert_eq!(out.status.code().unwrap_or(-1), 1, "a bad fixture is a config error");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("arguments object") && stderr.contains("an array"),
        "the error must say what was expected and what was found:\n{stderr}"
    );
}
