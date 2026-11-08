//! The commands that name their database print it without the password in the URL.
//!
//! Each command is run with a URL that carries a password and points at a port nothing
//! listens on: it prints what it is about to do, then fails to connect.

use std::process::Command;

/// The password in the URL every command is given. Nothing they print may contain it.
const PASSWORD: &str = "pw-cli-5d27";

fn credentialed_url() -> String {
    format!("postgres://fraiseql_app:{PASSWORD}@127.0.0.1:1/appdb")
}

/// stdout and stderr of `fraiseql-cli args…`.
fn output(args: &[&str]) -> String {
    let out = Command::new(env!("CARGO_BIN_EXE_fraiseql-cli"))
        .args(args)
        .output()
        .expect("run fraiseql-cli");
    format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    )
}

fn assert_names_the_database_without_its_password(command: &str, output: &str) {
    assert!(
        output.contains("Database:"),
        "`{command}` no longer names its database:\n{output}"
    );
    assert!(
        output.contains("127.0.0.1:1/appdb"),
        "`{command}` no longer names the host and database:\n{output}"
    );
    assert!(
        !output.contains(PASSWORD),
        "`{command}` printed the database password:\n{output}"
    );
}

#[test]
fn introspect_facts_names_the_database_without_its_password() {
    let url = credentialed_url();
    let out = output(&["introspect", "facts", "--database", &url]);
    assert_names_the_database_without_its_password("introspect facts", &out);
}

#[test]
fn validate_facts_names_the_database_without_its_password() {
    let scratch = tempfile::tempdir().expect("scratch dir");
    let schema = scratch.path().join("schema.json");
    std::fs::write(&schema, r#"{"types": [], "queries": [], "mutations": []}"#)
        .expect("write schema");
    let url = credentialed_url();
    let out = output(&[
        "validate",
        "facts",
        "--schema",
        schema.to_str().expect("a UTF-8 temp path"),
        "--database",
        &url,
    ]);
    assert_names_the_database_without_its_password("validate facts", &out);
}

#[test]
fn setup_dry_run_names_the_database_without_its_password() {
    let url = credentialed_url();
    let out = output(&["setup", "--dry-run", "--database", &url]);
    assert!(
        out.contains("Database URL:"),
        "`setup --dry-run` no longer names its database:\n{out}"
    );
    assert!(
        out.contains("fraiseql setup --database 'postgres://fraiseql_app:***@127.0.0.1:1/appdb'"),
        "`setup --dry-run` no longer prints the command to apply:\n{out}"
    );
    assert!(
        !out.contains(PASSWORD),
        "`setup --dry-run` printed the database password:\n{out}"
    );
}
