#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable
#![allow(missing_docs)]

//! Integration test for #329 — session variables MUST be visible to the SQL
//! function that the mutation runner calls.
//!
//! ## The bug
//!
//! `PostgresAdapter::set_session_variables` acquires its own pooled connection
//! and runs `SELECT set_config($1, $2, true)` per variable. `is_local = true`
//! makes the setting transaction-scoped, but a bare `SELECT` is its own
//! autocommit transaction — so the GUC is discarded the instant the statement
//! returns. Even setting that aside, the subsequent `execute_function_call`
//! acquires a *different* pooled connection, so the function never sees the
//! value.
//!
//! These tests run against a real PostgreSQL container so the connection-pool
//! and transaction-scope behaviour is exercised exactly as in production.

mod common;

use fraiseql_core::db::{DatabaseAdapter, postgres::PostgresAdapter};

/// Install a function that echoes a transaction-local GUC back to the caller.
/// Returns a fresh adapter (its own pool) connected to the shared container.
async fn adapter_with_echo_fn() -> PostgresAdapter {
    let container = common::testcontainer::get_test_container().await;
    let adapter = PostgresAdapter::new(&container.connection_string())
        .await
        .expect("connect adapter");

    adapter
        .execute_raw_query(
            "CREATE OR REPLACE FUNCTION fn_show_tenant_329() RETURNS text \
             LANGUAGE sql AS $$ SELECT current_setting('app.tenant_id_329', true) $$;",
        )
        .await
        .expect("create echo function");

    adapter
}

/// #329 — the value applied via session variables must be visible to the
/// function body.
///
/// On v2.3.2 this fails with `None` ("<NULL>") because the GUC is applied on a
/// different connection / transaction than the one that runs the function.
#[tokio::test]
async fn session_variables_visible_inside_function() {
    let adapter = adapter_with_echo_fn().await;

    // The two-call pattern the executor uses today: apply the session
    // variables, then call the function. This is exactly bug #329.
    adapter
        .set_session_variables(&[("app.tenant_id_329", "tenant-abc")])
        .await
        .unwrap();
    let rows = adapter
        .execute_function_call("fn_show_tenant_329", &[])
        .await
        .unwrap();

    let value = rows[0].get("fn_show_tenant_329").and_then(|v| v.as_str());

    assert_eq!(
        value,
        Some("tenant-abc"),
        "expected app.tenant_id_329 == \"tenant-abc\" inside fn_show_tenant_329, \
         got {value:?} — this is bug #329 (GUC observed as NULL inside the function)"
    );
}
