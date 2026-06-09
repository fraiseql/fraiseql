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
    assert!(PgCatalog::connect("mysql://localhost/db").is_err(), "non-postgres URL rejected");
}

/// `table_columns` reads column names + `udt_name` for the exact PostgreSQL
/// base types the change-log contract drift check (#380) compares against.
///
/// Uses a uniquely-named probe table in `public` (created + dropped here) so it
/// never touches the shared `core.tb_entity_change_log` other suites depend on.
#[tokio::test]
async fn table_columns_reads_name_and_udt() {
    const PROBE: &str = "public.tb_changelog_drift_probe_380";

    let Some(url) = fraiseql_test_support::try_database_url() else {
        return;
    };

    // Setup/teardown DDL goes over a raw connection — PgCatalog is read-only.
    let (client, conn) = match tokio_postgres::connect(&url, tokio_postgres::NoTls).await {
        Ok(pair) => pair,
        Err(e) => {
            eprintln!("skipping #380 table_columns test: {e}");
            return;
        },
    };
    tokio::spawn(async move {
        let _ = conn.await;
    });

    client
        .batch_execute(&format!(
            "DROP TABLE IF EXISTS {PROBE}; \
             CREATE TABLE {PROBE} ( \
                object_id uuid, \
                label     text, \
                fk_thing  bigint, \
                tags      text[] \
             );"
        ))
        .await
        .expect("create probe table");

    let catalog = PgCatalog::connect(&url).expect("connect catalog");
    let cols = catalog
        .table_columns("public", "tb_changelog_drift_probe_380")
        .await
        .expect("introspect probe table");

    // Drop before asserting so a failed assertion still leaves a clean DB.
    client
        .batch_execute(&format!("DROP TABLE IF EXISTS {PROBE};"))
        .await
        .expect("drop probe table");

    let by_name: std::collections::HashMap<&str, &str> =
        cols.iter().map(|c| (c.name.as_str(), c.udt_name.as_str())).collect();
    assert_eq!(cols.len(), 4, "all four probe columns are read: {cols:?}");
    assert_eq!(by_name.get("object_id"), Some(&"uuid"), "uuid → udt_name uuid");
    assert_eq!(by_name.get("label"), Some(&"text"), "text → udt_name text");
    assert_eq!(by_name.get("fk_thing"), Some(&"int8"), "bigint → udt_name int8");
    assert_eq!(by_name.get("tags"), Some(&"_text"), "text[] → udt_name _text");
}

/// An absent table introspects to an empty column list — the drift check reads
/// that as "table not found, the migration will install it".
#[tokio::test]
async fn table_columns_absent_table_is_empty() {
    let Some(catalog) = catalog().await else {
        return;
    };
    let cols = catalog
        .table_columns("core", "tb_a_table_that_does_not_exist_380")
        .await
        .expect("introspection of an absent table must not error");
    assert!(cols.is_empty(), "absent table → empty column list, got: {cols:?}");
}
