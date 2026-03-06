#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable
//! Integration tests for `fraiseql migrate`.
//!
//! The `migrate` command wraps the external `confiture` tool.  In CI (where
//! confiture is not installed) these tests verify the CLI's error path and
//! the help output.  The `create` sub-command is also tested in isolation
//! because it only requires file-system access.
//!
//! **Execution engine:** none (CLI binary only)
//! **Infrastructure:** none (filesystem only; confiture not required)
//! **Parallelism:** safe

use std::process::Command;

fn cli() -> Command {
    Command::new(env!("CARGO_BIN_EXE_fraiseql-cli"))
}

/// Returns true if `confiture` is available in PATH.
fn confiture_available() -> bool {
    Command::new("confiture")
        .arg("--version")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .is_ok_and(|s| s.success())
}

// ── Help output ───────────────────────────────────────────────────────────────

/// `fraiseql migrate --help` exits 0 and describes sub-commands.
#[test]
fn migrate_help_exits_zero() {
    let out = cli().args(["migrate", "--help"]).output().unwrap();
    assert!(out.status.success(), "migrate --help must exit 0");
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(
        text.contains("up") || text.contains("down") || text.contains("status"),
        "migrate --help must describe sub-commands; got: {text}"
    );
}

/// `fraiseql migrate up --help` exits 0.
#[test]
fn migrate_up_help_exits_zero() {
    let out = cli().args(["migrate", "up", "--help"]).output().unwrap();
    assert!(out.status.success(), "migrate up --help must exit 0");
}

/// `fraiseql migrate create --help` exits 0.
#[test]
fn migrate_create_help_exits_zero() {
    let out = cli().args(["migrate", "create", "--help"]).output().unwrap();
    assert!(out.status.success(), "migrate create --help must exit 0");
}

// ── Error path: confiture not installed ───────────────────────────────────────

/// `migrate up` without confiture installed exits non-zero and prints instructions.
#[test]
fn migrate_up_without_confiture_exits_nonzero() {
    if confiture_available() {
        return; // skip: confiture present
    }
    let out = cli()
        .args(["migrate", "up", "--database", "postgres://localhost/test"])
        .output()
        .unwrap();
    assert!(
        !out.status.success(),
        "migrate up without confiture must exit non-zero"
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("confiture") || stderr.contains("install"),
        "error message must mention confiture or install; got: {stderr}"
    );
}

/// `migrate status` without confiture and no database URL exits non-zero.
#[test]
fn migrate_status_no_database_url_exits_nonzero() {
    if confiture_available() {
        return; // skip: confiture present — needs real DB
    }
    // Run in a temp dir with no fraiseql.toml so the URL is unresolvable
    let tmp = tempfile::tempdir().unwrap();
    let out = cli()
        .current_dir(tmp.path())
        .args(["migrate", "status"])
        .env_remove("DATABASE_URL")
        .output()
        .unwrap();
    assert!(
        !out.status.success(),
        "migrate status with no URL and no confiture must exit non-zero"
    );
}

// ── `migrate create` sub-command ─────────────────────────────────────────────

/// `migrate create <name>` without confiture exits non-zero (confiture needed for create).
#[test]
fn migrate_create_without_confiture_exits_nonzero() {
    if confiture_available() {
        return; // skip: behavior differs when confiture is present
    }
    let tmp = tempfile::tempdir().unwrap();
    let out = cli()
        .args(["migrate", "create", "add_posts_table", "--dir", tmp.path().to_str().unwrap()])
        .output()
        .unwrap();
    assert!(
        !out.status.success(),
        "migrate create without confiture must exit non-zero"
    );
}

// ── `fraiseql.toml` URL resolution ───────────────────────────────────────────

/// A `fraiseql.toml` with `[database].url` is used if no `--database` flag given.
/// (Tests the resolve_database_url helper indirectly via the CLI.)
/// This test just checks the error path is well-formed, not that a DB connection succeeds.
#[test]
fn migrate_reads_database_url_from_toml() {
    if confiture_available() {
        return; // with confiture, a real connection would be attempted
    }
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(
        tmp.path().join("fraiseql.toml"),
        "[database]\nurl = \"postgres://localhost/toml_test\"\n",
    )
    .unwrap();
    let out = cli()
        .current_dir(tmp.path())
        .args(["migrate", "up"])
        .output()
        .unwrap();
    // Without confiture this will fail, but the error should NOT be "no database URL"
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        !stderr.contains("No database URL"),
        "URL should have been resolved from fraiseql.toml; stderr: {stderr}"
    );
}
