//! `fraiseql migrate` drives the confiture the integration leg pins, not the confiture a
//! document describes.
//!
//! Before this binary existed, `fraiseql migrate status` shelled `confiture status --source
//! DIR`. Confiture has no top-level `status` — its migration verbs live under `confiture
//! migrate` — and no `--source` option on any verb, so every call the wrapper made (`up`,
//! `down`, `status`, `create`, `validate`) died with confiture's "No such command" (exit 2).
//! The wrapper's only tests, in `migrate_command_tests.rs`, self-skip whenever confiture is
//! present; a wrapper whose tests run only where the wrapped tool is absent is not tested.
//!
//! Each test here runs the real CLI binary against the real confiture that the postgres
//! integration leg installs from `tools/confiture-requirements.txt`, and asserts on the exit
//! status and on confiture's own JSON report. The binary self-skips only on a missing
//! `DATABASE_URL` (the rig convention: inert in the database-free leg). Where the rig binds
//! a database, a missing `confiture` is a failure, not a skip — the skip is the shape that
//! hid the bug.
//!
//! **Execution engine:** `PostgreSQL`
//! **Infrastructure:** `DATABASE_URL`, `confiture` on `PATH`

#![cfg(feature = "test-postgres")]
#![allow(clippy::unwrap_used, clippy::print_stderr, clippy::panic)]
// Reason: test code — panics and skip diagnostics are acceptable

use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Output},
};

/// Version stamp of the one migration each test seeds. Confiture's file convention is
/// `<version>_<name>.up.sql` (14-digit timestamp).
const MIGRATION_VERSION: &str = "20260101000000";
const MIGRATION_NAME: &str = "probe";

fn cli() -> Command {
    Command::new(env!("CARGO_BIN_EXE_fraiseql-cli"))
}

/// The database this leg binds, or `None` to self-skip (database-free leg).
fn database_url_or_skip(test: &str) -> Option<String> {
    let url = fraiseql_test_support::try_database_url();
    if url.is_none() {
        eprintln!("skipping {test}: no DATABASE_URL");
    }
    url
}

/// The confiture on `PATH`, as `confiture --version` reports it.
///
/// Panics when it is absent: this binary runs only where `DATABASE_URL` is bound, and that
/// leg installs confiture from `tools/confiture-requirements.txt`. A skip here would
/// recreate the gap that let the wrapper shell non-existent commands for two releases.
fn confiture_version() -> String {
    match Command::new("confiture").arg("--version").output() {
        Ok(out) if out.status.success() => String::from_utf8_lossy(&out.stdout).trim().to_string(),
        other => panic!(
            "DATABASE_URL is bound, so this leg must also put confiture on PATH \
             (tools/confiture-requirements.txt); `confiture --version` gave {other:?}"
        ),
    }
}

/// Seeds `<root>/db/migrations` with one well-formed SQL migration; returns the directory.
fn seed_migrations(root: &Path) -> PathBuf {
    let dir = root.join("db").join("migrations");
    fs::create_dir_all(&dir).unwrap();
    fs::write(
        dir.join(format!("{MIGRATION_VERSION}_{MIGRATION_NAME}.up.sql")),
        "CREATE TABLE IF NOT EXISTS tb_migrate_against_confiture_probe (id integer);\n",
    )
    .unwrap();
    dir
}

fn assert_exit_zero(out: &Output, what: &str, confiture: &str) {
    assert!(
        out.status.success(),
        "{what} must exit 0 against {confiture}; exit {:?}\nstdout:\n{}\nstderr:\n{}",
        out.status.code(),
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr),
    );
}

fn parse_report(out: &Output, what: &str) -> serde_json::Value {
    let stdout = String::from_utf8_lossy(&out.stdout);
    serde_json::from_str(stdout.trim()).unwrap_or_else(|e| {
        panic!("{what} under --json must print one JSON document: {e}\nstdout:\n{stdout}")
    })
}

/// `fraiseql --json migrate status` exits 0 and prints confiture's status report, which
/// lists the seeded migration by version.
#[test]
fn migrate_status_runs_against_the_pinned_confiture() {
    let Some(url) = database_url_or_skip("migrate_status_runs_against_the_pinned_confiture") else {
        return;
    };
    let confiture = confiture_version();
    let tmp = tempfile::tempdir().unwrap();
    let migrations = seed_migrations(tmp.path());

    let out = cli()
        .current_dir(tmp.path())
        .args([
            "--json",
            "migrate",
            "status",
            "--dir",
            migrations.to_str().unwrap(),
        ])
        .env("DATABASE_URL", &url)
        .output()
        .unwrap();

    assert_exit_zero(&out, "fraiseql --json migrate status", &confiture);
    let report = parse_report(&out, "fraiseql migrate status");
    let versions: Vec<&str> = report["migrations"]
        .as_array()
        .unwrap_or_else(|| panic!("status report has no `migrations` array: {report}"))
        .iter()
        .filter_map(|m| m["version"].as_str())
        .collect();
    assert!(
        versions.contains(&MIGRATION_VERSION),
        "status report must list the seeded migration {MIGRATION_VERSION}; got {versions:?}"
    );
}

/// `fraiseql --json migrate create NAME` exits 0 and a migration named NAME appears in the
/// directory — confiture's verb for this is `migrate generate`.
#[test]
fn migrate_create_writes_a_migration_through_confiture() {
    let Some(url) = database_url_or_skip("migrate_create_writes_a_migration_through_confiture")
    else {
        return;
    };
    let confiture = confiture_version();
    let tmp = tempfile::tempdir().unwrap();
    let migrations = seed_migrations(tmp.path());

    let out = cli()
        .current_dir(tmp.path())
        .args([
            "--json",
            "migrate",
            "create",
            "add_probe_column",
            "--dir",
            migrations.to_str().unwrap(),
        ])
        .env("DATABASE_URL", &url)
        .output()
        .unwrap();

    assert_exit_zero(&out, "fraiseql --json migrate create", &confiture);
    let created: Vec<String> = fs::read_dir(&migrations)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .filter(|name| name.contains("add_probe_column"))
        .collect();
    assert_eq!(
        created.len(),
        1,
        "exactly one migration named add_probe_column must be created; found {created:?}"
    );
}

/// `fraiseql --json migrate validate` exits 0 on a well-named migration set and prints
/// confiture's validation report.
#[test]
fn migrate_validate_runs_against_the_pinned_confiture() {
    let Some(url) = database_url_or_skip("migrate_validate_runs_against_the_pinned_confiture")
    else {
        return;
    };
    let confiture = confiture_version();
    let tmp = tempfile::tempdir().unwrap();
    let migrations = seed_migrations(tmp.path());

    let out = cli()
        .current_dir(tmp.path())
        .args([
            "--json",
            "migrate",
            "validate",
            "--dir",
            migrations.to_str().unwrap(),
        ])
        .env("DATABASE_URL", &url)
        .output()
        .unwrap();

    assert_exit_zero(&out, "fraiseql --json migrate validate", &confiture);
    let report = parse_report(&out, "fraiseql migrate validate");
    assert_eq!(
        report["status"].as_str(),
        Some("ok"),
        "validation report must carry status ok; got {report}"
    );
}
