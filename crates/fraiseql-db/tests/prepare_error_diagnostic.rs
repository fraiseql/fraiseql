#![cfg(feature = "postgres")]
#![allow(clippy::unwrap_used, clippy::print_stderr, clippy::panic)] // Reason: test code, panics are acceptable

//! Regression proof for #451: a statement-prepare failure must surface the real
//! PostgreSQL diagnostic, not the opaque `db error`.
//!
//! `prepare_cached_stmt` is the single prepare-error mapping site on the
//! function-call path. Calling a function that does not exist makes PostgreSQL
//! fail the *prepare* (Parse) with SQLSTATE `42883` ("function ... does not
//! exist"). Before the fix the surfaced message was
//! `Failed to prepare statement: db error` — the `e.as_db_error()` detail was
//! discarded. This test asserts the human-readable diagnostic now rides through.
//!
//! Runs against the harness-provided PostgreSQL (Dagger-bound in CI via the
//! `integrationPostgres` leg, or a local spawn with the `local-testcontainers`
//! feature). It touches no shared tables, so it is isolation-safe.

use fraiseql_db::{DatabaseAdapter, PostgresAdapter};
use fraiseql_error::FraiseQLError;

#[tokio::test]
async fn prepare_failure_surfaces_postgres_diagnostic() {
    let svc = fraiseql_test_support::postgres()
        .await
        .expect("DATABASE_URL must be set (or enable fraiseql-test-support/local-testcontainers)");
    let adapter = PostgresAdapter::new(svc.url()).await.expect("build adapter");

    // A function that cannot exist — the prepare of `SELECT * FROM fn()` fails at
    // Parse time with SQLSTATE 42883 before any execution happens.
    let missing_fn = "fraiseql_test_nonexistent_fn_42883";
    let err = adapter
        .execute_function_call(missing_fn, &[])
        .await
        .expect_err("calling a non-existent function must fail");

    match err {
        FraiseQLError::Database { message, sql_state } => {
            // The real PostgreSQL diagnostic must be present...
            assert!(
                message.contains("does not exist"),
                "expected the PostgreSQL diagnostic in the message, got: {message:?}"
            );
            assert!(
                message.contains(missing_fn),
                "expected the offending function name in the message, got: {message:?}"
            );
            // ...and the opaque top-level Display must no longer be the detail.
            assert!(
                !message.ends_with("db error"),
                "message still ends with the opaque `db error`: {message:?}"
            );
            // SQLSTATE was already populated; assert it pins the function-not-found code.
            assert_eq!(sql_state.as_deref(), Some("42883"), "expected SQLSTATE 42883");
        },
        other => panic!("expected FraiseQLError::Database, got: {other:?}"),
    }
}
