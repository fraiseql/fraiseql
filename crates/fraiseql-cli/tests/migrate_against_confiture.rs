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
        Ok(out) if out.status.success() => String::from_utf8_lossy(&out.stdout)
            .lines()
            .next()
            .unwrap_or_default()
            .to_string(),
        other => panic!(
            "DATABASE_URL is bound, so this leg must also put confiture on PATH \
             (tools/confiture-requirements.txt); `confiture --version` gave {other:?}"
        ),
    }
}

/// Seeds `<root>/db/migrations` with one well-formed, reversible SQL migration; returns the
/// directory. The `.down.sql` sibling is what lets `migrate down` undo `migrate up`, so the
/// database this leg binds is left as it was found.
fn seed_migrations(root: &Path) -> PathBuf {
    let dir = root.join("db").join("migrations");
    fs::create_dir_all(&dir).unwrap();
    fs::write(
        dir.join(format!("{MIGRATION_VERSION}_{MIGRATION_NAME}.up.sql")),
        "CREATE TABLE IF NOT EXISTS tb_migrate_against_confiture_probe (id integer);\n",
    )
    .unwrap();
    fs::write(
        dir.join(format!("{MIGRATION_VERSION}_{MIGRATION_NAME}.down.sql")),
        "DROP TABLE IF EXISTS tb_migrate_against_confiture_probe;\n",
    )
    .unwrap();
    dir
}

/// The `status` field confiture's status report gives the seeded migration.
fn seeded_migration_status(report: &serde_json::Value) -> String {
    report["migrations"]
        .as_array()
        .unwrap_or_else(|| panic!("status report has no `migrations` array: {report}"))
        .iter()
        .find(|m| m["version"].as_str() == Some(MIGRATION_VERSION))
        .and_then(|m| m["status"].as_str())
        .unwrap_or_else(|| {
            panic!("status report must list the seeded migration {MIGRATION_VERSION}: {report}")
        })
        .to_string()
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

/// With only `DATABASE_URL` set — the way every rig and most shells hand a DSN over —
/// `fraiseql migrate up` applies the seeded migration, `status` then reports it `applied`,
/// `down` rolls it back, and `status` reports it `pending` again. Each step exits 0.
///
/// The DSN fraiseql resolves must be the one confiture connects with. Handed over as an
/// ambient `DATABASE_URL` with no `--no-config`, confiture's own connection ladder never
/// takes it: `status` reports every migration "unknown (no config)" and exits 0, which
/// looks like success, and `up`/`down` refuse with `CONFIG_010`. Only a mutating verb
/// followed by a status read proves a connection was made.
#[test]
fn migrate_up_then_status_reports_the_seeded_migration_applied() {
    let Some(url) =
        database_url_or_skip("migrate_up_then_status_reports_the_seeded_migration_applied")
    else {
        return;
    };
    let confiture = confiture_version();
    let tmp = tempfile::tempdir().unwrap();
    let migrations = seed_migrations(tmp.path());
    let migrate = |verb_args: &[&str]| {
        let mut args = vec!["--json", "migrate"];
        args.extend_from_slice(verb_args);
        args.extend_from_slice(&["--dir", migrations.to_str().unwrap()]);
        cli()
            .current_dir(tmp.path())
            .args(args)
            .env("DATABASE_URL", &url)
            .output()
            .unwrap()
    };

    let up = migrate(&["up"]);
    assert_exit_zero(&up, "fraiseql --json migrate up", &confiture);

    let status = migrate(&["status"]);
    assert_exit_zero(&status, "fraiseql --json migrate status after up", &confiture);
    assert_eq!(
        seeded_migration_status(&parse_report(&status, "fraiseql migrate status")),
        "applied",
        "after `migrate up`, status must report the seeded migration applied — \
         \"unknown\" means confiture never connected to the DSN fraiseql resolved"
    );

    let down = migrate(&["down", "--steps", "1"]);
    assert_exit_zero(&down, "fraiseql --json migrate down --steps 1", &confiture);

    // Confiture's `migrate status` exits 1 when migrations are pending — its reference,
    // §"confiture migrate status", "Exit Codes" — and the wrapper passes a non-zero exit
    // through as its own 1. The report is still confiture's, and it must say `pending`.
    let status = migrate(&["status"]);
    let report = parse_report(&status, "fraiseql migrate status after down");
    assert_eq!(
        seeded_migration_status(&report),
        "pending",
        "after `migrate down`, status must report the seeded migration pending again"
    );
    assert_eq!(
        status.status.code(),
        Some(1),
        "confiture {confiture} exits 1 on a pending set (\"pending migrations exist\") and \
         the wrapper passes that through; a different exit means the contract moved:\n{report}"
    );
}

/// `fraiseql --json migrate create NAME` exits 0 and one migration module named NAME appears
/// in the directory — confiture's verb for this is `migrate generate`. Since 1.19.0 generate
/// also writes a `NAME.verify.sql` sidecar next to the module; the sidecar is not the module.
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
        .filter(|name| name.contains("add_probe_column") && !name.ends_with(".verify.sql"))
        .collect();
    assert_eq!(
        created.len(),
        1,
        "exactly one migration module named add_probe_column must be created; found {created:?}"
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
