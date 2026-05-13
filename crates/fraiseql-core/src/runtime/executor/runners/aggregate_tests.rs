//! Tests for RLS enforcement in aggregate and window query paths.

#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

use std::collections::HashMap;
use std::sync::Arc;

use chrono::Utc;

use crate::{
    compiler::fact_table::{DimensionColumn, FactTableMetadata, FilterColumn, MeasureColumn, SqlType},
    runtime::{Executor, RuntimeConfig},
    security::{DefaultRLSPolicy, SecurityContext},
};
use crate::runtime::executor::test_support::CapturingMockAdapter;

fn tenant_security_context(tenant_id: &str) -> SecurityContext {
    SecurityContext {
        user_id:          "user-42".into(),
        roles:            vec!["viewer".to_string()],
        tenant_id:        Some(tenant_id.into()),
        scopes:           vec![],
        attributes:       HashMap::default(),
        request_id:       "req-001".to_string(),
        ip_address:       None,
        expires_at:       Utc::now() + chrono::Duration::hours(1),
        authenticated_at: Utc::now(),
        issuer:           None,
        audience:         None,
    }
}

fn admin_security_context() -> SecurityContext {
    SecurityContext {
        user_id:          "admin-1".into(),
        roles:            vec!["admin".to_string()],
        tenant_id:        Some("tenant-abc".into()),
        scopes:           vec![],
        attributes:       HashMap::default(),
        request_id:       "req-002".to_string(),
        ip_address:       None,
        expires_at:       Utc::now() + chrono::Duration::hours(1),
        authenticated_at: Utc::now(),
        issuer:           None,
        audience:         None,
    }
}

/// Build a schema with a `tf_sales` fact table that includes `tenant_id` as a
/// denormalized filter column, so RLS can produce direct-column WHERE clauses.
fn schema_with_fact_table() -> crate::schema::CompiledSchema {
    let mut schema = crate::schema::CompiledSchema::new();
    schema.add_fact_table(
        "tf_sales".to_string(),
        FactTableMetadata {
            table_name:           "tf_sales".to_string(),
            measures:             vec![MeasureColumn {
                name:     "revenue".to_string(),
                sql_type: SqlType::Decimal,
                nullable: false,
            }],
            dimensions:           DimensionColumn {
                name:  "data".to_string(),
                paths: vec![],
            },
            denormalized_filters: vec![
                FilterColumn {
                    name:     "tenant_id".to_string(),
                    sql_type: SqlType::Text,
                    indexed:  true,
                },
                FilterColumn {
                    name:     "author_id".to_string(),
                    sql_type: SqlType::Text,
                    indexed:  true,
                },
            ],
            calendar_dimensions:  vec![],
            partial_period:       None,
        },
    );
    schema
}

// ── Aggregate RLS tests ─────────────────────────────────────────────────────

#[tokio::test]
async fn aggregate_query_with_rls_includes_tenant_filter_in_sql() {
    let schema = schema_with_fact_table();
    let adapter = Arc::new(CapturingMockAdapter::new(vec![]));
    let config = RuntimeConfig::default().with_rls_policy(Arc::new(DefaultRLSPolicy::new()));
    let executor = Executor::with_config(schema, adapter.clone(), config);

    let ctx = tenant_security_context("tenant-abc");
    let vars = serde_json::json!({ "table": "tf_sales", "aggregates": [{"count": {}}] });
    let _result = executor
        .execute_with_security("{ sales_aggregate }", Some(&vars), &ctx)
        .await
        .unwrap();

    let sql = adapter.captured_aggregate_sql().expect("aggregate SQL should be captured");
    assert!(
        sql.contains("tenant_id"),
        "RLS tenant filter must appear in aggregate SQL, got: {sql}"
    );
}

#[tokio::test]
async fn aggregate_query_admin_bypasses_rls() {
    let schema = schema_with_fact_table();
    let adapter = Arc::new(CapturingMockAdapter::new(vec![]));
    let config = RuntimeConfig::default().with_rls_policy(Arc::new(DefaultRLSPolicy::new()));
    let executor = Executor::with_config(schema, adapter.clone(), config);

    let ctx = admin_security_context();
    let vars = serde_json::json!({ "table": "tf_sales", "aggregates": [{"count": {}}] });
    let _result = executor
        .execute_with_security("{ sales_aggregate }", Some(&vars), &ctx)
        .await
        .unwrap();

    let sql = adapter.captured_aggregate_sql().expect("aggregate SQL should be captured");
    // Admin should bypass RLS — no tenant_id filter in SQL
    assert!(
        !sql.contains("tenant_id"),
        "admin should bypass RLS, but SQL contains tenant_id: {sql}"
    );
}

#[tokio::test]
async fn aggregate_query_no_rls_policy_returns_unfiltered() {
    let schema = schema_with_fact_table();
    let adapter = Arc::new(CapturingMockAdapter::new(vec![]));
    // No RLS policy configured
    let executor = Executor::new(schema, adapter.clone());

    let ctx = tenant_security_context("tenant-abc");
    let vars = serde_json::json!({ "table": "tf_sales", "aggregates": [{"count": {}}] });
    let _result = executor
        .execute_with_security("{ sales_aggregate }", Some(&vars), &ctx)
        .await
        .unwrap();

    let sql = adapter.captured_aggregate_sql().expect("aggregate SQL should be captured");
    // No RLS policy means no tenant filter
    assert!(
        !sql.contains("tenant_id"),
        "without RLS policy, SQL should not contain tenant_id: {sql}"
    );
}

#[tokio::test]
async fn aggregate_rls_composes_with_user_where() {
    let schema = schema_with_fact_table();
    let adapter = Arc::new(CapturingMockAdapter::new(vec![]));
    let config = RuntimeConfig::default().with_rls_policy(Arc::new(DefaultRLSPolicy::new()));
    let executor = Executor::with_config(schema, adapter.clone(), config);

    let ctx = tenant_security_context("tenant-abc");
    // User-supplied WHERE on a denormalized filter
    let vars = serde_json::json!({
        "table": "tf_sales",
        "aggregates": [{"count": {}}],
        "where": {"tenant_id": {"eq": "tenant-abc"}}
    });
    let _result = executor
        .execute_with_security("{ sales_aggregate }", Some(&vars), &ctx)
        .await
        .unwrap();

    let sql = adapter.captured_aggregate_sql().expect("aggregate SQL should be captured");
    // Both RLS and user WHERE should be present (AND-composed)
    assert!(
        sql.contains("WHERE"),
        "combined WHERE expected in SQL: {sql}"
    );
    assert!(
        sql.contains("AND"),
        "RLS + user WHERE should be AND-composed: {sql}"
    );
}

// ── Window RLS tests ────────────────────────────────────────────────────────

#[tokio::test]
async fn window_query_with_rls_includes_tenant_filter_in_sql() {
    let schema = schema_with_fact_table();
    let adapter = Arc::new(CapturingMockAdapter::new(vec![]));
    let config = RuntimeConfig::default().with_rls_policy(Arc::new(DefaultRLSPolicy::new()));
    let executor = Executor::with_config(schema, adapter.clone(), config);

    let ctx = tenant_security_context("tenant-abc");
    let vars = serde_json::json!({
        "table": "tf_sales",
        "select": [{"type": "measure", "name": "revenue", "alias": "revenue"}],
        "windows": [{
            "function": {"type": "row_number"},
            "alias": "rank",
            "orderBy": [{"field": "revenue", "direction": "DESC"}]
        }]
    });
    let _result = executor
        .execute_with_security("{ sales_window }", Some(&vars), &ctx)
        .await
        .unwrap();

    let sql = adapter.captured_aggregate_sql().expect("window SQL should be captured");
    assert!(
        sql.contains("tenant_id"),
        "RLS tenant filter must appear in window SQL, got: {sql}"
    );
}

#[tokio::test]
async fn window_query_admin_bypasses_rls() {
    let schema = schema_with_fact_table();
    let adapter = Arc::new(CapturingMockAdapter::new(vec![]));
    let config = RuntimeConfig::default().with_rls_policy(Arc::new(DefaultRLSPolicy::new()));
    let executor = Executor::with_config(schema, adapter.clone(), config);

    let ctx = admin_security_context();
    let vars = serde_json::json!({
        "table": "tf_sales",
        "select": [{"type": "measure", "name": "revenue", "alias": "revenue"}],
        "windows": [{
            "function": {"type": "row_number"},
            "alias": "rank",
            "orderBy": [{"field": "revenue", "direction": "DESC"}]
        }]
    });
    let _result = executor
        .execute_with_security("{ sales_window }", Some(&vars), &ctx)
        .await
        .unwrap();

    let sql = adapter.captured_aggregate_sql().expect("window SQL should be captured");
    assert!(
        !sql.contains("tenant_id"),
        "admin should bypass RLS in window queries, but SQL contains tenant_id: {sql}"
    );
}
