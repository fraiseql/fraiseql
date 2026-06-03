#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable
#![allow(missing_docs)]

//! Integration tests for #329 — session variables MUST be visible to the SQL
//! function / view that the executor's adapter call runs.
//!
//! ## The bug
//!
//! `PostgresAdapter::set_session_variables` acquired its own pooled connection
//! and ran `SELECT set_config($1, $2, true)` per variable. `is_local = true`
//! makes the setting transaction-scoped, but a bare `SELECT` is its own
//! autocommit transaction — so the GUC was discarded the instant the statement
//! returned. Even setting that aside, the subsequent `execute_function_call` /
//! read acquired a *different* pooled connection, so the operation never saw
//! the value.
//!
//! The fix applies session variables transaction-locally on the *same*
//! connection as the operation via the `*_with_session` adapter methods.
//!
//! These tests run against a real PostgreSQL container so the connection-pool
//! and transaction-scope behaviour is exercised exactly as in production. Each
//! test uses a unique object/GUC suffix so the shared container can run them in
//! parallel without `CREATE OR REPLACE` racing on `pg_proc`/`pg_class`.

mod common;

use fraiseql_core::db::{DatabaseAdapter, RelayDatabaseAdapter, postgres::PostgresAdapter};

/// Connect a fresh adapter (its own pool) to the shared container and install a
/// function `fn_show_tenant_<key>()` that echoes the GUC `app.tenant_id_<key>`.
async fn adapter_with_echo_fn(key: &str) -> PostgresAdapter {
    let container = common::testcontainer::get_test_container().await;
    let adapter = PostgresAdapter::new(&container.connection_string())
        .await
        .expect("connect adapter");

    adapter
        .execute_raw_query(&format!(
            "CREATE OR REPLACE FUNCTION fn_show_tenant_{key}() RETURNS text \
             LANGUAGE sql AS $$ SELECT current_setting('app.tenant_id_{key}', true) $$;"
        ))
        .await
        .expect("create echo function");

    adapter
}

/// #329 — the connection-affine method applies the GUC on the same connection /
/// transaction as the function call, so the function body sees it.
#[tokio::test]
async fn function_call_with_session_sees_set_config() {
    let adapter = adapter_with_echo_fn("fncall").await;

    let rows = adapter
        .execute_function_call_with_session(
            "fn_show_tenant_fncall",
            &[],
            &[("app.tenant_id_fncall", "tenant-abc")],
        )
        .await
        .unwrap();

    let value = rows[0].get("fn_show_tenant_fncall").and_then(|v| v.as_str());
    assert_eq!(value, Some("tenant-abc"));
}

/// Set up an RLS-style table + view filtering on `current_setting`, returning
/// `(id, data)` so it works for both plain reads and relay pagination.
async fn setup_widget_view(key: &str) -> PostgresAdapter {
    let container = common::testcontainer::get_test_container().await;
    let adapter = PostgresAdapter::new(&container.connection_string())
        .await
        .expect("connect adapter");

    // execute_raw_query uses the extended protocol (one statement per call).
    for stmt in [
        format!(
            "CREATE TABLE IF NOT EXISTS tb_widget_{key} \
             (id bigint primary key, tenant text not null)"
        ),
        format!("TRUNCATE tb_widget_{key}"),
        format!(
            "INSERT INTO tb_widget_{key} VALUES (1, 'tenant-a'), (2, 'tenant-b'), (3, 'tenant-a')"
        ),
        format!(
            "CREATE OR REPLACE VIEW v_widget_{key} AS \
             SELECT id, jsonb_build_object('id', id, 'tenant', tenant) AS data \
             FROM tb_widget_{key} \
             WHERE tenant = current_setting('app.tenant_id_{key}', true)"
        ),
    ] {
        adapter.execute_raw_query(&stmt).await.unwrap();
    }

    adapter
}

/// Connection-affine read path: an RLS-style view filtering on
/// `current_setting` returns only the matching tenant's rows.
#[tokio::test]
async fn where_query_with_session_applies_rls_setting() {
    let adapter = setup_widget_view("where").await;

    let rows = adapter
        .execute_where_query_arc_with_session(
            "v_widget_where",
            None,
            None,
            None,
            None,
            &[("app.tenant_id_where", "tenant-a")],
        )
        .await
        .unwrap();

    assert_eq!(rows.len(), 2, "RLS-style view should return only tenant-a's rows");
    for row in rows.iter() {
        let tenant = row.as_value().pointer("/tenant").and_then(|v| v.as_str());
        assert_eq!(tenant, Some("tenant-a"));
    }
}

/// Connection-affine relay path: cursor pagination over an RLS-style view sees
/// the session variable, so only the matching tenant's rows (and an accurate
/// `totalCount`) are returned.
#[tokio::test]
async fn relay_page_with_session_applies_rls_setting() {
    let adapter = setup_widget_view("relay").await;

    let page = adapter
        .execute_relay_page_with_session(
            "v_widget_relay",
            "id",
            None,
            None,
            10,
            true,
            None,
            None,
            true, // include_total_count
            &[("app.tenant_id_relay", "tenant-a")],
        )
        .await
        .unwrap();

    assert_eq!(page.rows().len(), 2, "relay page should contain only tenant-a's rows");
    assert_eq!(page.total_count(), Some(2), "totalCount must respect RLS via session vars");
    for row in page.rows() {
        let tenant = row.as_value().pointer("/tenant").and_then(|v| v.as_str());
        assert_eq!(tenant, Some("tenant-a"));
    }

    // Sanity: without the session variable the view filters everything out.
    let empty = adapter
        .execute_relay_page_with_session(
            "v_widget_relay",
            "id",
            None,
            None,
            10,
            true,
            None,
            None,
            false,
            &[],
        )
        .await
        .unwrap();
    assert_eq!(empty.rows().len(), 0, "no session var => RLS predicate is NULL => no rows");
}
