#![allow(clippy::unwrap_used, clippy::panic)] // Reason: test code, panics acceptable
//! A fraiseql.toml that fails to parse once its `${VAR}` placeholders are expanded, through
//! the real binary.
//!
//! The error names the line and the reason, and shows neither the line's text nor a
//! variable's value: the expanded text may hold a secret, and the compiler's output reaches
//! CI logs. A file with no placeholder expanded keeps the full parse error, line text included.
//!
//! **Execution engine:** none · **Infrastructure:** none · **Parallelism:** safe

use std::{fs, process::Command};

use tempfile::TempDir;

/// A value no output may show.
const VALUE: &str = "secretSENTINEL";

/// `fraiseql compile fraiseql.toml --types types.json` with `toml` after the `[project]`
/// table (which ends on line 4), and `FQ_PARSE` set to [`VALUE`]. Whether it succeeded, and
/// its output.
fn compile(toml: &str) -> (bool, String) {
    compile_with(toml, VALUE)
}

/// [`compile`] with `FQ_PARSE` set to `value`.
fn compile_with(toml: &str, value: &str) -> (bool, String) {
    compile_os(toml, std::ffi::OsStr::new(value))
}

/// [`compile`] with `FQ_PARSE` set to `value`, which need not be UTF-8.
fn compile_os(toml: &str, value: &std::ffi::OsStr) -> (bool, String) {
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
    let output = Command::new(env!("CARGO_BIN_EXE_fraiseql-cli"))
        .current_dir(dir.path())
        .args([
            "compile",
            "fraiseql.toml",
            "--types",
            "types.json",
            "-o",
            "out.json",
        ])
        .env("FQ_PARSE", value)
        .output()
        .unwrap();
    let log = format!(
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    (output.status.success(), log)
}

#[test]
fn a_value_that_is_not_toml_is_reported_by_line_without_the_value() {
    let (ok, log) =
        compile("[security.rate_limiting]\nenabled = true\nauth_start_max_requests = ${FQ_PARSE}");
    assert!(!ok, "{log}");
    assert!(log.contains("line 7"), "names the line\n{log}");
    assert!(!log.contains(VALUE), "shows the value\n{log}");
    assert!(!log.contains("auth_start_max_requests ="), "shows the line's text\n{log}");
}

#[test]
fn a_value_of_the_wrong_type_is_reported_by_line_without_the_value() {
    let (ok, log) = compile(
        "[security.rate_limiting]\nenabled = true\nauth_start_max_requests = \"${FQ_PARSE}\"",
    );
    assert!(!ok, "{log}");
    assert!(log.contains("line 7"), "names the line\n{log}");
    assert!(!log.contains(VALUE), "shows the value\n{log}");
}

#[test]
fn a_file_with_nothing_expanded_keeps_the_whole_parse_error() {
    let (ok, log) =
        compile("[security.rate_limiting]\nenabled = true\nauth_start_max_requests = plainword");
    assert!(!ok, "{log}");
    assert!(log.contains("auth_start_max_requests = plainword"), "shows the line\n{log}");
}

/// One character of each class a value may not hold refuses the file, inside a quoted string,
/// naming the variable and the class and not the value; so does a value that is not UTF-8.
#[test]
fn a_value_that_is_not_printable_text_is_refused_by_class() {
    use std::{ffi::OsString, os::unix::ffi::OsStringExt};

    let toml = "[security.rate_limiting]\nenabled = true\nredis_url = \"redis://${FQ_PARSE}\"";
    for (value, class) in [
        (OsString::from("s3cretPART\tSENT"), "control character"),
        (OsString::from("s3cretPART\u{200b}SENT"), "format character"),
        (OsString::from("s3cretPART\u{a0}SENT"), "separator other than the plain space"),
        (OsString::from_vec(b"s3cretPART\xffSENT".to_vec()), "is not UTF-8"),
    ] {
        let (ok, log) = compile_os(toml, &value);
        assert!(!ok, "{value:?} compiled\n{log}");
        assert!(log.contains("Environment variable FQ_PARSE"), "{value:?}\n{log}");
        assert!(log.contains(class), "{value:?}: names the class\n{log}");
        assert!(!log.contains("s3cretPART"), "{value:?}: shows the value\n{log}");
    }
}

/// Printable text, accents and other scripts included, compiles as written.
#[test]
fn printable_text_compiles() {
    let toml = "[security.rate_limiting]\nenabled = true\nredis_url = \"redis://${FQ_PARSE}\"";
    let (ok, log) = compile_with(toml, "café über 東京");
    assert!(ok, "{log}");
}
