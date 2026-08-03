//! #384 — `compile --database` must be a drift linter that can FAIL.
//!
//! The view-composition linter (L1 relation existence, L2 column shape, L3
//! JSONB key sampling) and the mutation→function contract check have existed
//! since `d3e2f188a`/`829e66db9`, but every finding was advisory: `warn!` +
//! exit 0, with the compiled artifact written as if the schema were clean.
//! That is the #818/#820/#821 "validator that cannot fail" class.
//!
//! This suite pins the failure semantics against a real database:
//!
//! - error-severity drift (missing relation, required field's JSONB key absent from every sampled
//!   row, missing backing function) fails the compile with a non-zero exit and writes NO artifact;
//! - a clean schema↔database pair compiles green (the linter does not cry wolf);
//! - `--allow-drift` restores the advisory behaviour, loudly;
//! - warn-severity drift (a nullable field's key absent from samples) never fails the compile;
//! - `doctor --against-db --json` reports the same view drift as structured `Fail` checks, so CI
//!   can consume the report (acceptance: JSON output mode).
//!
//! Self-skips when no `DATABASE_URL` is set.
//!
//! **Execution engine:** `PostgreSQL`
//! **Infrastructure:** `DATABASE_URL`

#![cfg(feature = "test-postgres")]
#![allow(clippy::unwrap_used, clippy::print_stderr, clippy::panic)]
// Reason: test code — panics and skip diagnostics are acceptable

use std::{io::Write, process::Command};

use serde_json::json;
use tempfile::{Builder, NamedTempFile, TempDir};
use tokio_postgres::NoTls;

/// Connect to the rig database, or `None` (skip) when unavailable.
async fn client() -> Option<tokio_postgres::Client> {
    let url = fraiseql_test_support::try_database_url()?;
    match tokio_postgres::connect(&url, NoTls).await {
        Ok((client, conn)) => {
            tokio::spawn(async move {
                let _ = conn.await;
            });
            Some(client)
        },
        Err(e) => {
            eprintln!("skipping #384 drift test: cannot connect ({e})");
            None
        },
    }
}

/// (Re-)create a one-row jsonb view named `view` whose `data` column holds
/// `payload`. Views are cheap and self-contained — no backing table needed.
async fn create_jsonb_view(client: &tokio_postgres::Client, view: &str, payload: &str) {
    client
        .batch_execute(&format!(
            "DROP VIEW IF EXISTS {view};
             CREATE VIEW {view} AS
             SELECT gen_random_uuid() AS id, '{payload}'::jsonb AS data;"
        ))
        .await
        .unwrap();
}

/// Write an authoring (intermediate) schema declaring one type + one query
/// against `sql_source`, with the `author` field's nullability chosen by the
/// caller. The `author` key is the drift probe: present or absent in the view.
fn schema_file(sql_source: &str, author_nullable: bool) -> NamedTempFile {
    let schema = json!({
        "types": [{
            "name": "Thing",
            "sql_source": sql_source,
            "fields": [
                {"name": "id", "type": "ID", "nullable": false},
                {"name": "author", "type": "String", "nullable": author_nullable}
            ]
        }],
        "queries": [{
            "name": "things",
            "return_type": "Thing",
            "returns_list": true,
            "sql_source": sql_source
        }]
    });
    write_json(&schema)
}

fn write_json(value: &serde_json::Value) -> NamedTempFile {
    let mut f = Builder::new().suffix(".json").tempfile().unwrap();
    f.write_all(serde_json::to_string_pretty(value).unwrap().as_bytes()).unwrap();
    f.flush().unwrap();
    f
}

/// Run `fraiseql-cli compile <schema> --database <url> -o <out>` plus `extra`
/// flags; returns (success, combined stdout+stderr).
fn run_compile(
    schema_path: &std::path::Path,
    url: &str,
    out: &std::path::Path,
    extra: &[&str],
) -> (bool, String) {
    let mut args = vec![
        "compile".to_string(),
        schema_path.to_str().unwrap().to_string(),
        "--database".to_string(),
        url.to_string(),
        "--skip-hash".to_string(),
        "-o".to_string(),
        out.to_str().unwrap().to_string(),
    ];
    args.extend(extra.iter().map(ToString::to_string));
    let output = Command::new(env!("CARGO_BIN_EXE_fraiseql-cli"))
        .args(&args)
        .output()
        .expect("failed to run fraiseql-cli");
    let combined = format!(
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    (output.status.success(), combined)
}

/// A required field whose JSONB key is absent from every sampled row is
/// error-severity drift: the compile fails and no artifact is written.
#[tokio::test]
async fn drifted_required_key_fails_compile_and_writes_nothing() {
    let Some(client) = client().await else { return };
    let url = fraiseql_test_support::try_database_url().unwrap();

    // The comment's headline shape: the type declares `author`, the view
    // carries `author_id`.
    create_jsonb_view(&client, "v_cdf384_req_drift", r#"{"author_id": "a-1"}"#).await;

    let schema = schema_file("v_cdf384_req_drift", false);
    let out_dir = TempDir::new().unwrap();
    let out = out_dir.path().join("schema.compiled.json");

    let (ok, log) = run_compile(schema.path(), &url, &out, &[]);
    assert!(
        !ok,
        "compile must FAIL on a required field whose JSONB key is missing from every \
         sampled row; it exited 0. log:\n{log}"
    );
    assert!(
        log.contains("author"),
        "the failure must name the drifted field `author`. log:\n{log}"
    );
    assert!(
        log.contains("v_cdf384_req_drift"),
        "the failure must name the drifted view. log:\n{log}"
    );
    assert!(
        !out.exists(),
        "a failed compile must not write the compiled artifact (watch mode relies on \
         the previous good artifact surviving a bad save)"
    );
}

/// A clean schema↔view pair compiles green — the linter does not cry wolf.
#[tokio::test]
async fn clean_schema_compiles_green() {
    let Some(client) = client().await else { return };
    let url = fraiseql_test_support::try_database_url().unwrap();

    create_jsonb_view(&client, "v_cdf384_clean", r#"{"author": "ada", "title": "t"}"#).await;

    let schema = schema_file("v_cdf384_clean", false);
    let out_dir = TempDir::new().unwrap();
    let out = out_dir.path().join("schema.compiled.json");

    let (ok, log) = run_compile(schema.path(), &url, &out, &[]);
    assert!(ok, "a clean schema↔view pair must compile. log:\n{log}");
    assert!(out.exists(), "a green compile must write the artifact");
}

/// `--allow-drift` restores the advisory behaviour: exit 0, artifact written,
/// finding still reported.
#[tokio::test]
async fn allow_drift_downgrades_errors_to_advisories() {
    let Some(client) = client().await else { return };
    let url = fraiseql_test_support::try_database_url().unwrap();

    create_jsonb_view(&client, "v_cdf384_allow", r#"{"author_id": "a-1"}"#).await;

    let schema = schema_file("v_cdf384_allow", false);
    let out_dir = TempDir::new().unwrap();
    let out = out_dir.path().join("schema.compiled.json");

    let (ok, log) = run_compile(schema.path(), &url, &out, &["--allow-drift"]);
    assert!(ok, "--allow-drift must let the compile through. log:\n{log}");
    assert!(out.exists(), "--allow-drift must write the artifact");
    assert!(
        log.contains("author"),
        "--allow-drift must still report the drift, not swallow it. log:\n{log}"
    );
}

/// A `sql_source` that resolves to no relation is error-severity drift.
#[tokio::test]
async fn missing_relation_fails_compile() {
    let Some(_client) = client().await else {
        return;
    };
    let url = fraiseql_test_support::try_database_url().unwrap();

    let schema = schema_file("v_cdf384_absent", false);
    let out_dir = TempDir::new().unwrap();
    let out = out_dir.path().join("schema.compiled.json");

    let (ok, log) = run_compile(schema.path(), &url, &out, &[]);
    assert!(!ok, "compile must FAIL when sql_source resolves to no relation. log:\n{log}");
    assert!(
        log.contains("v_cdf384_absent"),
        "the failure must name the missing relation. log:\n{log}"
    );
    assert!(!out.exists(), "a failed compile must not write the artifact");
}

/// A NULLABLE field whose key is absent from samples is a warning, not an
/// error — optional fields may legitimately be unset in every sampled row.
#[tokio::test]
async fn nullable_field_absent_key_warns_but_compiles() {
    let Some(client) = client().await else { return };
    let url = fraiseql_test_support::try_database_url().unwrap();

    create_jsonb_view(&client, "v_cdf384_nullable", r#"{"title": "t"}"#).await;

    let schema = schema_file("v_cdf384_nullable", true);
    let out_dir = TempDir::new().unwrap();
    let out = out_dir.path().join("schema.compiled.json");

    let (ok, log) = run_compile(schema.path(), &url, &out, &[]);
    assert!(
        ok,
        "a nullable field's absent key is advisory; the compile must pass. log:\n{log}"
    );
    assert!(out.exists(), "warn-severity drift must still write the artifact");
}

/// A mutation whose backing function does not exist is error-severity contract
/// drift (comment category 3) and fails the compile.
#[tokio::test]
async fn mutation_with_no_backing_function_fails_compile() {
    let Some(client) = client().await else { return };
    let url = fraiseql_test_support::try_database_url().unwrap();

    create_jsonb_view(&client, "v_cdf384_mut", r#"{"author": "ada"}"#).await;

    let schema = json!({
        "types": [{
            "name": "Thing",
            "sql_source": "v_cdf384_mut",
            "fields": [
                {"name": "id", "type": "ID", "nullable": false},
                {"name": "author", "type": "String", "nullable": false}
            ]
        }],
        "queries": [{
            "name": "things",
            "return_type": "Thing",
            "returns_list": true,
            "sql_source": "v_cdf384_mut"
        }],
        "mutations": [{
            "name": "createThing",
            "return_type": "Thing",
            "sql_source": "fn_cdf384_absent",
            "operation": "INSERT",
            "arguments": [{"name": "author", "type": "String", "nullable": false}]
        }]
    });
    let schema = write_json(&schema);
    let out_dir = TempDir::new().unwrap();
    let out = out_dir.path().join("schema.compiled.json");

    let (ok, log) = run_compile(schema.path(), &url, &out, &[]);
    assert!(
        !ok,
        "compile must FAIL when a declared mutation has no backing function. log:\n{log}"
    );
    assert!(
        log.contains("fn_cdf384_absent"),
        "the failure must name the missing function. log:\n{log}"
    );
    assert!(!out.exists(), "a failed compile must not write the artifact");
}

/// `doctor --against-db --json` reports view drift as structured `Fail`
/// checks and exits non-zero — the CI-consumable form of the same linter.
#[tokio::test]
async fn doctor_reports_view_drift_as_structured_fail() {
    let Some(client) = client().await else { return };
    let url = fraiseql_test_support::try_database_url().unwrap();

    create_jsonb_view(&client, "v_cdf384_doctor", r#"{"author_id": "a-1"}"#).await;

    // Produce a compiled artifact for doctor via --allow-drift (doctor consumes
    // compiled schemas, and this one is deliberately drifted).
    let schema = schema_file("v_cdf384_doctor", false);
    let out_dir = TempDir::new().unwrap();
    let out = out_dir.path().join("schema.compiled.json");
    let (ok, log) = run_compile(schema.path(), &url, &out, &["--allow-drift"]);
    assert!(ok, "--allow-drift compile must succeed to feed doctor. log:\n{log}");

    let mut config = Builder::new().suffix(".toml").tempfile().unwrap();
    config.write_all(b"[server]\nbind = \"0.0.0.0:8000\"\n").unwrap();
    config.flush().unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_fraiseql-cli"))
        .args([
            "doctor",
            "--config",
            config.path().to_str().unwrap(),
            "--schema",
            out.to_str().unwrap(),
            "--against-db",
            &url,
            "--json",
        ])
        .output()
        .expect("failed to run fraiseql-cli doctor");

    assert!(
        !output.status.success(),
        "doctor must exit non-zero on view drift. stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    let json_start = stdout.find('[').expect("doctor --json must print a JSON array");
    let checks: serde_json::Value = serde_json::from_str(stdout[json_start..].trim())
        .expect("doctor --json must be valid JSON");
    let drift_fail = checks.as_array().unwrap().iter().any(|c| {
        c["status"] == "fail"
            && c["detail"].as_str().unwrap_or_default().contains("v_cdf384_doctor")
    });
    assert!(
        drift_fail,
        "doctor --json must contain a Fail check naming the drifted view. got:\n{checks:#}"
    );
}

/// #384 category 2 e2e: a real plpgsql function that extracts one payload key
/// but not another declared input field. Warn-severity: the compile passes and
/// the log names the dropped field.
#[tokio::test]
async fn unreferenced_input_field_warns_but_compiles() {
    let Some(client) = client().await else { return };
    let url = fraiseql_test_support::try_database_url().unwrap();

    create_jsonb_view(&client, "v_cdf384_payload", r#"{"author": "ada"}"#).await;
    client
        .batch_execute(
            "DROP FUNCTION IF EXISTS fn_cdf384_update(jsonb);
             CREATE FUNCTION fn_cdf384_update(payload jsonb)
             RETURNS TABLE(succeeded boolean, state_changed boolean, message text)
             LANGUAGE plpgsql AS $$
             BEGIN
               RETURN QUERY SELECT true, true, payload->>'author';
             END $$;",
        )
        .await
        .unwrap();

    let schema = json!({
        "types": [{
            "name": "Thing",
            "sql_source": "v_cdf384_payload",
            "fields": [
                {"name": "id", "type": "ID", "nullable": false},
                {"name": "author", "type": "String", "nullable": false}
            ]
        }],
        "input_types": [{
            "name": "UpdateThingInput",
            "fields": [
                {"name": "author", "type": "String", "nullable": true},
                {"name": "ownerId", "type": "String", "nullable": true}
            ]
        }],
        "queries": [{
            "name": "things",
            "return_type": "Thing",
            "returns_list": true,
            "sql_source": "v_cdf384_payload"
        }],
        "mutations": [{
            "name": "updateThing",
            "return_type": "Thing",
            "sql_source": "fn_cdf384_update",
            "operation": "UPDATE",
            "arguments": [{"name": "input", "type": "UpdateThingInput", "nullable": false}]
        }]
    });
    let schema = write_json(&schema);
    let out_dir = TempDir::new().unwrap();
    let out = out_dir.path().join("schema.compiled.json");

    let (ok, log) = run_compile(schema.path(), &url, &out, &[]);
    assert!(
        ok,
        "an unreferenced input field is warn-grade — the compile must pass. log:\n{log}"
    );
    assert!(
        log.contains("ownerId") || log.contains("owner_id"),
        "the log must name the silently-dropped input field. log:\n{log}"
    );
    assert!(
        !log.contains("input field `author`"),
        "the extracted field `author` must not be flagged. log:\n{log}"
    );
}
