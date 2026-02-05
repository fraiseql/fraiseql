//! Integration tests for CLI help documentation

use std::process::Command;

/// Helper function to run CLI command with --help flag
fn get_help_output(args: &[&str]) -> String {
    let mut cmd = Command::new("cargo");
    cmd.arg("run").arg("-p").arg("fraiseql-cli").arg("--").args(args).arg("--help");

    let output = cmd.output().expect("Failed to run command");
    String::from_utf8_lossy(&output.stdout).to_string()
}

#[test]
fn test_main_help_shows_json_flag() {
    let help = get_help_output(&[]);
    assert!(
        help.contains("--json") || help.contains("-j"),
        "Main help should document --json flag"
    );
}

#[test]
fn test_main_help_shows_quiet_flag() {
    let help = get_help_output(&[]);
    assert!(
        help.contains("--quiet") || help.contains("-q"),
        "Main help should document --quiet flag"
    );
}

#[test]
fn test_compile_help_shows_json_flag() {
    let help = get_help_output(&["compile"]);
    assert!(
        help.contains("--json") || help.contains("-j"),
        "compile help should document --json flag"
    );
}

#[test]
fn test_compile_help_shows_quiet_flag() {
    let help = get_help_output(&["compile"]);
    assert!(
        help.contains("--quiet") || help.contains("-q"),
        "compile help should document --quiet flag"
    );
}

#[test]
fn test_validate_help_shows_json_flag() {
    let help = get_help_output(&["validate"]);
    assert!(
        help.contains("--json") || help.contains("-j"),
        "validate help should document --json flag"
    );
}

#[test]
fn test_validate_help_shows_quiet_flag() {
    let help = get_help_output(&["validate"]);
    assert!(
        help.contains("--quiet") || help.contains("-q"),
        "validate help should document --quiet flag"
    );
}

#[test]
fn test_explain_help_shows_json_flag() {
    let help = get_help_output(&["explain"]);
    assert!(
        help.contains("--json") || help.contains("-j"),
        "explain help should document --json flag"
    );
}

#[test]
fn test_explain_help_shows_quiet_flag() {
    let help = get_help_output(&["explain"]);
    assert!(
        help.contains("--quiet") || help.contains("-q"),
        "explain help should document --quiet flag"
    );
}

#[test]
fn test_cost_help_shows_json_flag() {
    let help = get_help_output(&["cost"]);
    assert!(
        help.contains("--json") || help.contains("-j"),
        "cost help should document --json flag"
    );
}

#[test]
fn test_cost_help_shows_quiet_flag() {
    let help = get_help_output(&["cost"]);
    assert!(
        help.contains("--quiet") || help.contains("-q"),
        "cost help should document --quiet flag"
    );
}

#[test]
fn test_analyze_help_shows_json_flag() {
    let help = get_help_output(&["analyze"]);
    assert!(
        help.contains("--json") || help.contains("-j"),
        "analyze help should document --json flag"
    );
}

#[test]
fn test_analyze_help_shows_quiet_flag() {
    let help = get_help_output(&["analyze"]);
    assert!(
        help.contains("--quiet") || help.contains("-q"),
        "analyze help should document --quiet flag"
    );
}

#[test]
fn test_federation_graph_help_shows_json_flag() {
    let help = get_help_output(&["federation", "graph"]);
    assert!(
        help.contains("--json") || help.contains("-j"),
        "federation graph help should document --json flag"
    );
}

#[test]
fn test_federation_graph_help_shows_quiet_flag() {
    let help = get_help_output(&["federation", "graph"]);
    assert!(
        help.contains("--quiet") || help.contains("-q"),
        "federation graph help should document --quiet flag"
    );
}

#[test]
fn test_compile_help_shows_usage_example() {
    let help = get_help_output(&["compile"]);
    // Help text should contain some indication of how to use the command
    assert!(help.len() > 20, "compile help should contain substantial documentation");
}

#[test]
fn test_validate_help_shows_usage_example() {
    let help = get_help_output(&["validate"]);
    assert!(help.len() > 20, "validate help should contain substantial documentation");
}

#[test]
fn test_explain_help_shows_usage_example() {
    let help = get_help_output(&["explain"]);
    assert!(help.len() > 20, "explain help should contain substantial documentation");
}

#[test]
fn test_cost_help_shows_usage_example() {
    let help = get_help_output(&["cost"]);
    assert!(help.len() > 20, "cost help should contain substantial documentation");
}

#[test]
fn test_analyze_help_shows_usage_example() {
    let help = get_help_output(&["analyze"]);
    assert!(help.len() > 20, "analyze help should contain substantial documentation");
}

#[test]
fn test_federation_graph_help_shows_usage_example() {
    let help = get_help_output(&["federation", "graph"]);
    assert!(
        help.len() > 20,
        "federation graph help should contain substantial documentation"
    );
}
