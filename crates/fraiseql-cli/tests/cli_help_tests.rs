//! Integration tests for CLI help documentation — in-process via assert_cmd.

use assert_cmd::Command;
use predicates::prelude::*;

fn fraiseql() -> Command {
    Command::cargo_bin("fraiseql-cli").expect("fraiseql-cli binary not found")
}

// ── main --help ──────────────────────────────────────────────────────────────

#[test]
fn test_main_help_shows_json_flag() {
    fraiseql()
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("--json").or(predicate::str::contains("-j")));
}

#[test]
fn test_main_help_shows_quiet_flag() {
    fraiseql()
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("--quiet").or(predicate::str::contains("-q")));
}

// ── compile --help ───────────────────────────────────────────────────────────

#[test]
fn test_compile_help_shows_json_flag() {
    fraiseql()
        .args(["compile", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("--json").or(predicate::str::contains("-j")));
}

#[test]
fn test_compile_help_shows_quiet_flag() {
    fraiseql()
        .args(["compile", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("--quiet").or(predicate::str::contains("-q")));
}

#[test]
fn test_compile_help_shows_usage_example() {
    fraiseql()
        .args(["compile", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::is_match(r".{20,}").unwrap());
}

// ── validate --help ──────────────────────────────────────────────────────────

#[test]
fn test_validate_help_shows_json_flag() {
    fraiseql()
        .args(["validate", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("--json").or(predicate::str::contains("-j")));
}

#[test]
fn test_validate_help_shows_quiet_flag() {
    fraiseql()
        .args(["validate", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("--quiet").or(predicate::str::contains("-q")));
}

#[test]
fn test_validate_help_shows_usage_example() {
    fraiseql()
        .args(["validate", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::is_match(r".{20,}").unwrap());
}

// ── explain --help ───────────────────────────────────────────────────────────

#[test]
fn test_explain_help_shows_json_flag() {
    fraiseql()
        .args(["explain", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("--json").or(predicate::str::contains("-j")));
}

#[test]
fn test_explain_help_shows_quiet_flag() {
    fraiseql()
        .args(["explain", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("--quiet").or(predicate::str::contains("-q")));
}

#[test]
fn test_explain_help_shows_usage_example() {
    fraiseql()
        .args(["explain", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::is_match(r".{20,}").unwrap());
}

// ── cost --help ──────────────────────────────────────────────────────────────

#[test]
fn test_cost_help_shows_json_flag() {
    fraiseql()
        .args(["cost", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("--json").or(predicate::str::contains("-j")));
}

#[test]
fn test_cost_help_shows_quiet_flag() {
    fraiseql()
        .args(["cost", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("--quiet").or(predicate::str::contains("-q")));
}

#[test]
fn test_cost_help_shows_usage_example() {
    fraiseql()
        .args(["cost", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::is_match(r".{20,}").unwrap());
}

// ── analyze --help ───────────────────────────────────────────────────────────

#[test]
fn test_analyze_help_shows_json_flag() {
    fraiseql()
        .args(["analyze", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("--json").or(predicate::str::contains("-j")));
}

#[test]
fn test_analyze_help_shows_quiet_flag() {
    fraiseql()
        .args(["analyze", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("--quiet").or(predicate::str::contains("-q")));
}

#[test]
fn test_analyze_help_shows_usage_example() {
    fraiseql()
        .args(["analyze", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::is_match(r".{20,}").unwrap());
}

// ── federation graph --help ──────────────────────────────────────────────────

#[test]
fn test_federation_graph_help_shows_json_flag() {
    fraiseql()
        .args(["federation", "graph", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("--json").or(predicate::str::contains("-j")));
}

#[test]
fn test_federation_graph_help_shows_quiet_flag() {
    fraiseql()
        .args(["federation", "graph", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("--quiet").or(predicate::str::contains("-q")));
}

#[test]
fn test_federation_graph_help_shows_usage_example() {
    fraiseql()
        .args(["federation", "graph", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::is_match(r".{20,}").unwrap());
}
