#![allow(clippy::unwrap_used, clippy::panic)] // Reason: test code, panics acceptable
//! A `${VAR}` placeholder written outside a quoted string in fraiseql.toml, through the real
//! binary.
//!
//! The placeholder is expanded in the file's text before it is parsed, so outside a quoted
//! string its value is read as bare TOML. There a `#` would end the line early, and a `,` or
//! `=` would add keys to the inline table it sits in, so a value holding `#`, `,`, `=` or
//! whitespace is refused, naming the variable and never the value. Inside a quoted string the
//! same value is text, and compiles as written.
//!
//! **Execution engine:** none · **Infrastructure:** none · **Parallelism:** safe

use std::{fs, process::Command};

use serde_json::Value;
use tempfile::TempDir;

/// The outcome of one compile: whether it succeeded, its output, and the compiled schema.
struct Run {
    ok:     bool,
    log:    String,
    schema: Option<Value>,
}

/// `fraiseql compile fraiseql.toml --types types.json` with `toml` after `[project]`, and
/// `FQ_PLACEHOLDER` set to `value`.
fn compile(toml: &str, value: &str) -> Run {
    let dir = TempDir::new().unwrap();
    fs::write(
        dir.path().join("types.json"),
        r#"{"types":[{"name":"Order","sql_source":"v_order",
            "fields":[{"name":"id","type":"ID","nullable":false}]}],
            "queries":[{"name":"orders","return_type":"Order","returns_list":true,
            "sql_source":"v_order"}],"mutations":[]}"#,
    )
    .unwrap();
    fs::write(
        dir.path().join("fraiseql.toml"),
        format!("[project]\nname = \"app\"\nversion = \"1.0.0\"\n\n{toml}\n"),
    )
    .unwrap();
    let out = dir.path().join("schema.compiled.json");
    let output = Command::new(env!("CARGO_BIN_EXE_fraiseql-cli"))
        .current_dir(dir.path())
        .args(["compile", "fraiseql.toml", "--types", "types.json", "-o"])
        .arg(&out)
        .env("FQ_PLACEHOLDER", value)
        .output()
        .unwrap();
    Run {
        ok:     output.status.success(),
        log:    format!(
            "{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        ),
        schema: fs::read_to_string(&out).ok().map(|s| serde_json::from_str(&s).unwrap()),
    }
}

/// Assert the compile was refused naming the variable, without showing `value`.
fn assert_refused(run: &Run, value: &str) {
    assert!(!run.ok, "{value:?} compiled\n{}", run.log);
    assert!(run.log.contains("FQ_PLACEHOLDER"), "{value:?}: names the variable\n{}", run.log);
    assert!(run.log.contains("outside a quoted string"), "{value:?}: says why\n{}", run.log);
    assert!(!run.log.contains(value), "{value:?}: shows the value\n{}", run.log);
}

#[test]
fn a_bare_value_holding_a_comment_is_refused_not_truncated() {
    let toml =
        "[security.rate_limiting]\nenabled = true\nauth_start_max_requests = ${FQ_PLACEHOLDER}";
    for value in ["120 # rest-SENTINEL", "120#rest-SENTINEL"] {
        assert_refused(&compile(toml, value), value);
    }
}

#[test]
fn a_bare_value_adding_an_inline_table_key_is_refused() {
    let toml = "[security]\nrate_limiting = { enabled = ${FQ_PLACEHOLDER} }";
    for value in [
        "true, auth_start_max_requests = 999999",
        "true,auth_start_window_secs=1",
        "true auth_start_window_secs",
    ] {
        assert_refused(&compile(toml, value), value);
    }
}

#[test]
fn a_bare_value_with_none_of_those_characters_compiles() {
    let toml =
        "[security.rate_limiting]\nenabled = true\nauth_start_max_requests = ${FQ_PLACEHOLDER}";
    let run = compile(toml, "120");
    assert!(run.ok, "{}", run.log);
    assert_eq!(run.schema.unwrap()["security"]["rate_limiting"]["auth_start_max_requests"], 120);
}

#[test]
fn the_same_value_inside_a_quoted_string_compiles_as_written() {
    for quoted in [
        "redis_url = \"redis://${FQ_PLACEHOLDER}:6379\"",
        "redis_url = 'redis://${FQ_PLACEHOLDER}:6379'",
        "redis_url = \"\"\"redis://${FQ_PLACEHOLDER}:6379\"\"\"",
        "redis_url = \"redis://\\\"${FQ_PLACEHOLDER}:6379\"",
    ] {
        let toml = format!("[security.rate_limiting]\nenabled = true\n{quoted}");
        let run = compile(&toml, "h # x, y = z");
        assert!(run.ok, "{quoted}\n{}", run.log);
        let url = run.schema.unwrap()["security"]["rate_limiting"]["redis_url"].clone();
        assert!(url.as_str().unwrap().contains("h # x, y = z"), "{quoted}: {url}");
    }
}

#[test]
fn a_placeholder_in_a_comment_is_left_alone() {
    let toml =
        "# auth_start_max_requests = ${FQ_PLACEHOLDER}\n[security.rate_limiting]\nenabled = true";
    let run = compile(toml, "1 # x, y = z");
    assert!(run.ok, "{}", run.log);
}
