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
    assert!(!out.status.success(), "migrate up without confiture must exit non-zero");
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
        .args([
            "migrate",
            "create",
            "add_posts_table",
            "--dir",
            tmp.path().to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(!out.status.success(), "migrate create without confiture must exit non-zero");
}

// ── New subcommands: help exits 0 ─────────────────────────────────────────────

/// `fraiseql migrate generate --help` exits 0.
#[test]
fn migrate_generate_help_exits_zero() {
    let out = cli().args(["migrate", "generate", "--help"]).output().unwrap();
    assert!(out.status.success(), "migrate generate --help must exit 0");
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(
        text.contains("NAME") || text.contains("name") || text.contains("migration"),
        "generate help must describe NAME argument; got: {text}"
    );
}

/// `fraiseql migrate validate --help` exits 0.
#[test]
fn migrate_validate_help_exits_zero() {
    let out = cli().args(["migrate", "validate", "--help"]).output().unwrap();
    assert!(out.status.success(), "migrate validate --help must exit 0");
}

/// `fraiseql migrate preflight --help` exits 0.
#[test]
fn migrate_preflight_help_exits_zero() {
    let out = cli().args(["migrate", "preflight", "--help"]).output().unwrap();
    assert!(out.status.success(), "migrate preflight --help must exit 0");
}

// ── New subcommands: error path without confiture ─────────────────────────────

/// `migrate generate <name>` without confiture exits non-zero.
#[test]
fn migrate_generate_without_confiture_exits_nonzero() {
    if confiture_available() {
        return; // skip: behavior differs when confiture is present
    }
    let tmp = tempfile::tempdir().unwrap();
    let out = cli()
        .args([
            "migrate",
            "generate",
            "add_posts_table",
            "--dir",
            tmp.path().to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(!out.status.success(), "migrate generate without confiture must exit non-zero");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("confiture") || stderr.contains("install"),
        "error must mention confiture; got: {stderr}"
    );
}

/// `migrate validate` without confiture exits non-zero.
#[test]
fn migrate_validate_without_confiture_exits_nonzero() {
    if confiture_available() {
        return;
    }
    let out = cli().args(["migrate", "validate"]).output().unwrap();
    assert!(!out.status.success(), "migrate validate without confiture must exit non-zero");
}

/// `migrate preflight` without confiture exits non-zero.
#[test]
fn migrate_preflight_without_confiture_exits_nonzero() {
    if confiture_available() {
        return;
    }
    let out = cli().args(["migrate", "preflight"]).output().unwrap();
    assert!(!out.status.success(), "migrate preflight without confiture must exit non-zero");
}

// ── `fraiseql compile --emit-ddl` ────────────────────────────────────────────

/// `compile --emit-ddl` writes DDL files to the specified directory.
#[test]
fn compile_emit_ddl_writes_files() {
    let tmp = tempfile::tempdir().unwrap();
    let ddl_dir = tmp.path().join("ddl");
    let schema_path =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/minimal_schema.json");
    let out_path = tmp.path().join("schema.compiled.json");

    let out = cli()
        .args([
            "compile",
            schema_path.to_str().unwrap(),
            "-o",
            out_path.to_str().unwrap(),
            "--emit-ddl",
            ddl_dir.to_str().unwrap(),
        ])
        .output()
        .unwrap();

    assert!(out.status.success(), "compile --emit-ddl must exit 0; stderr: {}", String::from_utf8_lossy(&out.stderr));
    assert!(ddl_dir.exists(), "DDL directory must be created");

    // minimal_schema.json has a User type → expect user.sql
    let user_sql = ddl_dir.join("user.sql");
    assert!(user_sql.exists(), "user.sql must be emitted; files: {:?}", std::fs::read_dir(&ddl_dir).unwrap().collect::<Vec<_>>());

    let content = std::fs::read_to_string(&user_sql).unwrap();
    assert!(content.contains("CREATE TABLE"), "DDL must contain CREATE TABLE; got: {content}");
    assert!(content.contains("tb_user"), "table name must be tb_user; got: {content}");
}

/// `compile --emit-ddl` DDL contains expected column types.
#[test]
fn compile_emit_ddl_column_types() {
    let tmp = tempfile::tempdir().unwrap();
    let ddl_dir = tmp.path().join("ddl");
    let schema_path =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/minimal_schema.json");
    let out_path = tmp.path().join("schema.compiled.json");

    let out = cli()
        .args([
            "compile",
            schema_path.to_str().unwrap(),
            "-o",
            out_path.to_str().unwrap(),
            "--emit-ddl",
            ddl_dir.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(out.status.success());

    let content = std::fs::read_to_string(ddl_dir.join("user.sql")).unwrap();
    // id is Int → INTEGER, email is String → TEXT
    assert!(content.contains("INTEGER"), "Int field must map to INTEGER; got: {content}");
    assert!(content.contains("TEXT"), "String field must map to TEXT; got: {content}");
}

// ── `fraiseql.toml` URL resolution ───────────────────────────────────────────

/// A `fraiseql.toml` with `[database].url` is used if no `--database` flag given.
/// (Tests the `resolve_database_url` helper indirectly via the CLI.)
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
    let out = cli().current_dir(tmp.path()).args(["migrate", "up"]).output().unwrap();
    // Without confiture this will fail, but the error should NOT be "no database URL"
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        !stderr.contains("No database URL"),
        "URL should have been resolved from fraiseql.toml; stderr: {stderr}"
    );
}
