//! Live-PostgreSQL integration test for `fraiseql setup` (#426).
//!
//! The embedded helper library (`sql/helpers/mutation_response.sql`) defines
//! dollar-quoted PL/pgSQL function bodies. The previous installer split the file
//! on `;` and executed fragments individually, which shredded those `$$…$$`
//! bodies and the trailing `DO`-block self-tests — so `fraiseql setup` failed on
//! a clean database and installed zero helpers. This test runs the real binary
//! against a database and asserts the helpers install and are callable.
//!
//! Self-skips when no `DATABASE_URL` is set, so it is inert in the database-free
//! test leg (even under `--all-features`).
//!
//! **Execution engine:** PostgreSQL
//! **Infrastructure:** `DATABASE_URL`
//! **Parallelism:** installs into the shared `fraiseql` schema via idempotent
//!   `CREATE OR REPLACE`; safe to repeat.
#![cfg(feature = "test-postgres")]
#![allow(clippy::unwrap_used, clippy::print_stderr)] // Reason: test code — panics and skip diagnostics are acceptable

use std::process::Command;

use tokio_postgres::NoTls;

#[tokio::test]
async fn setup_installs_dollar_quoted_helpers() {
    let Some(url) = fraiseql_test_support::try_database_url() else {
        eprintln!("skipping #426 setup against-db test: DATABASE_URL not set");
        return;
    };

    // Run the real installer. Before the fix this exits non-zero because the
    // `split(';')` loop produces broken SQL fragments on the first `$$` body.
    let out = Command::new(env!("CARGO_BIN_EXE_fraiseql-cli"))
        .args(["setup", "--database", &url])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "fraiseql setup must install dollar-quoted helpers and pass the file's \
         own DO-block self-tests; exit={:?}\nstderr:\n{}",
        out.status,
        String::from_utf8_lossy(&out.stderr)
    );

    // Verify the three helpers exist and are callable.
    let (client, connection) = tokio_postgres::connect(&url, NoTls).await.unwrap();
    tokio::spawn(async move {
        let _ = connection.await;
    });

    let version: String = client
        .query_one("SELECT fraiseql.library_version() AS v", &[])
        .await
        .unwrap()
        .get("v");
    assert_eq!(version, "2.2.0", "library_version() must report the installed version");

    // mutation_ok / mutation_err return the 13-column response and are callable.
    let ok_succeeded: bool = client
        .query_one("SELECT succeeded FROM fraiseql.mutation_ok('{\"id\":\"x\"}'::jsonb)", &[])
        .await
        .unwrap()
        .get("succeeded");
    assert!(ok_succeeded, "mutation_ok must return succeeded=true");

    let err_succeeded: bool = client
        .query_one("SELECT succeeded FROM fraiseql.mutation_err('not_found')", &[])
        .await
        .unwrap()
        .get("succeeded");
    assert!(!err_succeeded, "mutation_err must return succeeded=false");
}
