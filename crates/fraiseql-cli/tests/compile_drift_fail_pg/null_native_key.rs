//! The native-column agreement check (#1579) counts a SQL `NULL` column and a missing or
//! JSON-`null` document key as the same value.
//!
//! Both reads refuse such a row for every tenant: the root filter `tenant_id = $1` is not true
//! for a `NULL` column, and the embedded filter on `data->>'tenant_id'` is not true for a
//! JSON `null` or a missing key. A view that builds the key from the column therefore agrees on
//! a row with no tenant, and the compile accepts it. A key that is `null` or missing while the
//! column holds a value, or the reverse, still disagrees.

use super::*;

/// A view `name` over one row whose `tenant_id` column is `column` and whose document carries
/// `key` (a SQL expression for the `tenant_id` member, or `None` to leave the member out).
async fn create_view(client: &tokio_postgres::Client, name: &str, column: &str, key: Option<&str>) {
    let member = key.map_or_else(String::new, |key| format!(", 'tenant_id', {key}"));
    client
        .batch_execute(&format!(
            "DROP VIEW IF EXISTS {name};
             CREATE VIEW {name} AS SELECT t.id, t.tenant_id, \
             jsonb_build_object('id', t.id{member}) AS data \
             FROM (VALUES (1, {column}::uuid)) t(id, tenant_id);"
        ))
        .await
        .unwrap();
}

/// Whether `compile --database` accepts a schema scoped by `tenant_id` over `view`, and its log.
fn compile_over(url: &str, view: &str) -> (bool, String) {
    let schema = write_json(&json!({
        "types": [{
            "name": "AuditLog",
            "sql_source": view,
            "fields": [
                {"name": "id", "type": "Int", "nullable": false},
                {"name": "tenant_id", "type": "UUID", "nullable": true}
            ]
        }],
        "queries": [{
            "name": "auditLogs",
            "return_type": "AuditLog",
            "returns_list": true,
            "sql_source": view,
            "inject_params": {"tenant_id": "jwt:tenant_id"}
        }]
    }));
    let out_dir = TempDir::new().unwrap();
    run_compile(schema.path(), url, &out_dir.path().join("schema.compiled.json"), &[])
}

const TENANT: &str = "'00000000-0000-0000-0000-00000000000a'";

#[tokio::test]
async fn a_null_column_and_a_json_null_key_agree() {
    let Some(client) = client().await else { return };
    let url = fraiseql_test_support::try_database_url().unwrap();
    create_view(&client, "v_cdf1663_json_null", "NULL", Some("t.tenant_id")).await;
    let (ok, log) = compile_over(&url, "v_cdf1663_json_null");
    assert!(ok, "a NULL column whose key is built from it must compile:\n{log}");
}

#[tokio::test]
async fn a_null_column_and_a_missing_key_agree() {
    let Some(client) = client().await else { return };
    let url = fraiseql_test_support::try_database_url().unwrap();
    create_view(&client, "v_cdf1663_missing", "NULL", None).await;
    let (ok, log) = compile_over(&url, "v_cdf1663_missing");
    assert!(ok, "a NULL column and a missing key must compile:\n{log}");
}

#[tokio::test]
async fn a_json_null_key_beside_a_tenant_column_disagrees() {
    let Some(client) = client().await else { return };
    let url = fraiseql_test_support::try_database_url().unwrap();
    create_view(&client, "v_cdf1663_key_null", TENANT, Some("'null'::jsonb")).await;
    let (ok, log) = compile_over(&url, "v_cdf1663_key_null");
    assert!(!ok, "a JSON null key beside a tenant column must fail the compile:\n{log}");
    assert!(
        log.contains("1 of 1 sampled `v_cdf1663_key_null` rows"),
        "the failure counts the row:\n{log}"
    );
}

#[tokio::test]
async fn a_tenant_key_beside_a_null_column_disagrees() {
    let Some(client) = client().await else { return };
    let url = fraiseql_test_support::try_database_url().unwrap();
    create_view(&client, "v_cdf1663_column_null", "NULL", Some(TENANT)).await;
    let (ok, log) = compile_over(&url, "v_cdf1663_column_null");
    assert!(!ok, "a tenant key beside a NULL column must fail the compile:\n{log}");
    assert!(
        log.contains("1 of 1 sampled `v_cdf1663_column_null` rows"),
        "the failure counts the row:\n{log}"
    );
}
