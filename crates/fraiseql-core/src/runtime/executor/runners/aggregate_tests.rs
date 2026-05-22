//! Tests for RLS enforcement in aggregate and window query paths.

#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

use std::{collections::HashMap, sync::Arc};

use chrono::Utc;

use crate::{
    compiler::fact_table::{
        DimensionColumn, FactTableMetadata, FilterColumn, MeasureColumn, PartialPeriodConfig,
        SqlType, TemporalGrain,
    },
    runtime::{Executor, RuntimeConfig, executor::test_support::CapturingMockAdapter},
    security::{DefaultRLSPolicy, SecurityContext},
};

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
        email:            None,
        display_name:     None,
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
        email:            None,
        display_name:     None,
    }
}

/// Build a schema with a `tf_sales` fact table that includes `tenant_id` as a
/// denormalized filter column, so RLS can produce direct-column WHERE clauses.
fn schema_with_fact_table() -> crate::schema::CompiledSchema {
    let mut schema = crate::schema::CompiledSchema::new();
    schema.add_fact_table(
        "tf_sales".to_string(),
        FactTableMetadata {
            table_name:               "tf_sales".to_string(),
            measures:                 vec![MeasureColumn {
                name:     "revenue".to_string(),
                sql_type: SqlType::Decimal,
                nullable: false,
            }],
            dimensions:               DimensionColumn {
                name:  "data".to_string(),
                paths: vec![],
            },
            denormalized_filters:     vec![
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
            calendar_dimensions:      vec![],
            partial_period:           None,
            native_measures:          std::collections::HashMap::new(),
            native_dimension_mapping: std::collections::HashMap::new(),
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
    assert!(sql.contains("WHERE"), "combined WHERE expected in SQL: {sql}");
    assert!(sql.contains("AND"), "RLS + user WHERE should be AND-composed: {sql}");
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

// ── Partial-period dispatch tests ──────────────────────────────────────────

/// Build a schema with a fact table that has partial-period config.
fn schema_with_partial_period() -> crate::schema::CompiledSchema {
    let mut schema = crate::schema::CompiledSchema::new();
    schema.add_fact_table(
        "tf_events".to_string(),
        FactTableMetadata {
            table_name:               "tf_events".to_string(),
            measures:                 vec![MeasureColumn {
                name:     "volume".to_string(),
                sql_type: SqlType::BigInt,
                nullable: false,
            }],
            dimensions:               DimensionColumn {
                name:  "data".to_string(),
                paths: vec![],
            },
            denormalized_filters:     vec![
                FilterColumn {
                    name:     "tenant_id".to_string(),
                    sql_type: SqlType::Text,
                    indexed:  true,
                },
                FilterColumn {
                    name:     "period_start".to_string(),
                    sql_type: SqlType::Date,
                    indexed:  true,
                },
            ],
            calendar_dimensions:      vec![],
            partial_period:           Some(PartialPeriodConfig {
                fine_grain_view:   "v_events_day".to_string(),
                time_grain_column: "period_start".to_string(),
                time_grain_trunc:  TemporalGrain::Month,
            }),
            native_measures:          std::collections::HashMap::new(),
            native_dimension_mapping: std::collections::HashMap::new(),
        },
    );
    schema
}

#[tokio::test]
async fn partial_period_dispatch_generates_union_all() {
    let schema = schema_with_partial_period();
    let adapter = Arc::new(CapturingMockAdapter::new(vec![]));
    let executor = Executor::new(schema, adapter.clone());

    // Lower bound mid-month in the past → triggers partial-period UNION ALL
    let vars = serde_json::json!({
        "table": "tf_events",
        "aggregates": [{"count": {}}],
        "where": {"period_start_gte": "2020-01-15"}
    });
    let _result = executor.execute("{ events_aggregate }", Some(&vars)).await.unwrap();

    let sql = adapter.captured_aggregate_sql().expect("SQL should be captured");
    assert!(
        sql.contains("UNION ALL"),
        "partial-period dispatch should generate UNION ALL, got: {sql}"
    );
    assert!(sql.contains("v_events_day"), "fine-grain view should appear in SQL: {sql}");
}

#[tokio::test]
async fn partial_period_not_triggered_without_date_filter() {
    let schema = schema_with_partial_period();
    let adapter = Arc::new(CapturingMockAdapter::new(vec![]));
    let executor = Executor::new(schema, adapter.clone());

    // No date filter → standard aggregation path
    let vars = serde_json::json!({
        "table": "tf_events",
        "aggregates": [{"count": {}}],
    });
    let _result = executor.execute("{ events_aggregate }", Some(&vars)).await.unwrap();

    let sql = adapter.captured_aggregate_sql().expect("SQL should be captured");
    assert!(
        !sql.contains("UNION ALL"),
        "without date filter, should use standard path, got: {sql}"
    );
    assert!(
        !sql.contains("v_events_day"),
        "fine-grain view should NOT appear without date filter: {sql}"
    );
}

#[tokio::test]
async fn partial_period_with_rls_includes_tenant_in_all_branches() {
    let schema = schema_with_partial_period();
    let adapter = Arc::new(CapturingMockAdapter::new(vec![]));
    let config = RuntimeConfig::default().with_rls_policy(Arc::new(DefaultRLSPolicy::new()));
    let executor = Executor::with_config(schema, adapter.clone(), config);

    let ctx = tenant_security_context("tenant-abc");
    let vars = serde_json::json!({
        "table": "tf_events",
        "aggregates": [{"count": {}}],
        "where": {"period_start_gte": "2020-01-15"}
    });
    let _result = executor
        .execute_with_security("{ events_aggregate }", Some(&vars), &ctx)
        .await
        .unwrap();

    let sql = adapter.captured_aggregate_sql().expect("SQL should be captured");
    assert!(sql.contains("UNION ALL"), "should use partial-period path: {sql}");

    // RLS tenant filter should appear in EVERY branch
    let branches: Vec<&str> = sql.split("UNION ALL").collect();
    assert!(
        branches.len() >= 2,
        "expected at least 2 branches, got {}: {sql}",
        branches.len()
    );
    for (i, branch) in branches.iter().enumerate() {
        assert!(
            branch.contains("tenant_id"),
            "branch {} missing tenant_id RLS filter: {branch}",
            i + 1
        );
    }
}

#[tokio::test]
async fn partial_period_gt_operator_triggers_dispatch() {
    let schema = schema_with_partial_period();
    let adapter = Arc::new(CapturingMockAdapter::new(vec![]));
    let executor = Executor::new(schema, adapter.clone());

    // Use gt (exclusive) instead of gte — should be converted to next-day inclusive
    let vars = serde_json::json!({
        "table": "tf_events",
        "aggregates": [{"count": {}}],
        "where": {"period_start_gt": "2020-01-14"}
    });
    let _result = executor.execute("{ events_aggregate }", Some(&vars)).await.unwrap();

    let sql = adapter.captured_aggregate_sql().expect("SQL should be captured");
    assert!(
        sql.contains("UNION ALL"),
        "gt operator should trigger partial-period dispatch: {sql}"
    );
    // The params should contain "2020-01-15" (gt 14th → gte 15th)
    let params = adapter.captured_aggregate_params().expect("params should be captured");
    assert!(
        params.iter().any(|p| p == &serde_json::json!("2020-01-15")),
        "gt 2020-01-14 should produce gte 2020-01-15 in params: {:?}",
        params
    );
}
