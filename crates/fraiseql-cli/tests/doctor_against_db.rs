//! Live-PostgreSQL integration tests for the `doctor --against-db` PL/pgSQL
//! body-resolution pass (#409).
//!
//! The body-resolution pass depends on the `plpgsql_check` extension, which is
//! absent on stock Postgres images (including the CI image) and most managed
//! services. These tests assert the **graceful-degradation** contract: when the
//! extension is unavailable the pass reports
//! [`PlpgsqlCheckOutcome::Unavailable`] instead of erroring, and when it is
//! available it runs. The "found an unresolved call" happy path requires the
//! extension and is therefore not exercised here.
//!
//! Self-skips when no `DATABASE_URL` is set.

#![cfg(feature = "test-postgres")]
#![allow(clippy::unwrap_used, clippy::print_stderr)] // Reason: test code — panics and skip diagnostics are acceptable

use fraiseql_cli::schema::pg_catalog::{PgCatalog, PlpgsqlCheckOutcome};

async fn catalog() -> Option<PgCatalog> {
    let url = fraiseql_test_support::try_database_url()?;
    match PgCatalog::connect(&url) {
        Ok(c) => Some(c),
        Err(e) => {
            eprintln!("skipping #409 against-db test: {e}");
            None
        },
    }
}

#[tokio::test]
async fn body_resolution_degrades_gracefully_when_extension_absent() {
    let Some(catalog) = catalog().await else {
        return;
    };

    // Probe availability, then run the pass; the two must agree and the pass
    // must never error out (the whole point of #409's degradation path).
    let available = match catalog.plpgsql_check_available().await {
        Ok(a) => a,
        Err(e) => {
            eprintln!("skipping: cannot probe extensions ({e})");
            return;
        },
    };

    let outcome = catalog
        .plpgsql_check_unresolved_calls(&["public".to_string()])
        .await
        .expect("body-resolution pass must not error");

    if available {
        assert!(
            matches!(outcome, PlpgsqlCheckOutcome::Ran { .. }),
            "extension available → pass should run"
        );
    } else {
        assert_eq!(
            outcome,
            PlpgsqlCheckOutcome::Unavailable,
            "extension absent → pass should skip gracefully"
        );
    }
}

#[tokio::test]
async fn non_postgres_url_is_rejected() {
    // PgCatalog::connect rejects non-postgres URLs up front (the --against-db
    // checks are PostgreSQL-only).
    assert!(PgCatalog::connect("mysql://localhost/db").is_err());
}
