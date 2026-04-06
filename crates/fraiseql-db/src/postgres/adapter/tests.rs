//! Unit tests for the PostgreSQL adapter (no live database required).

#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

use fraiseql_error::FraiseQLError;

use super::{PoolPrewarmConfig, PostgresAdapter, build_where_select_sql, escape_jsonb_key};

// ── build_where_select_sql ─────────────────────────────────────────────────

#[test]
fn test_build_where_select_sql_no_clause() {
    let (sql, params) = build_where_select_sql("v_user", None, None, None).unwrap();
    assert_eq!(sql, r#"SELECT data FROM "v_user""#);
    assert!(params.is_empty());
}

#[test]
fn test_build_where_select_sql_with_limit_offset() {
    let (sql, params) = build_where_select_sql("v_order", None, Some(10), Some(20)).unwrap();
    // LIMIT takes $1, OFFSET takes $2.
    assert!(sql.contains("LIMIT $1"), "expected LIMIT $1 in: {sql}");
    assert!(sql.contains("OFFSET $2"), "expected OFFSET $2 in: {sql}");
    assert_eq!(params.len(), 2, "expected 2 params (limit + offset)");
}

#[test]
fn test_escape_jsonb_key_no_quotes() {
    assert_eq!(escape_jsonb_key("normal"), "normal");
    assert_eq!(escape_jsonb_key("created_at"), "created_at");
}

#[test]
fn test_escape_jsonb_key_doubles_single_quotes() {
    assert_eq!(escape_jsonb_key("it's"), "it''s");
    assert_eq!(escape_jsonb_key("a''b"), "a''''b");
}

// ── PoolPrewarmConfig struct ───────────────────────────────────────────────

#[test]
fn pool_prewarm_config_carries_all_fields() {
    let cfg = PoolPrewarmConfig { min_size: 5, max_size: 20, timeout_secs: Some(30) };
    assert_eq!(cfg.min_size, 5);
    assert_eq!(cfg.max_size, 20);
    assert_eq!(cfg.timeout_secs, Some(30));
}

#[test]
fn pool_prewarm_config_no_timeout_is_none() {
    let cfg = PoolPrewarmConfig { min_size: 0, max_size: 10, timeout_secs: None };
    assert!(cfg.timeout_secs.is_none());
}

#[test]
fn pool_prewarm_config_min_zero_is_valid() {
    let cfg = PoolPrewarmConfig { min_size: 0, max_size: 5, timeout_secs: None };
    assert_eq!(cfg.min_size, 0);
    assert_eq!(cfg.max_size, 5);
}

#[test]
fn pool_prewarm_config_min_equals_max_is_valid() {
    let cfg = PoolPrewarmConfig { min_size: 10, max_size: 10, timeout_secs: Some(60) };
    assert_eq!(cfg.min_size, cfg.max_size);
}

// ── EP-5: Connection pool failure paths ───────────────────────────────────

#[tokio::test]
async fn test_new_with_malformed_url_returns_connection_pool_error() {
    // A completely unparseable URL causes deadpool-postgres to fail immediately
    // at pool creation or the initial `pool.get()`, both mapped to ConnectionPool.
    let result = PostgresAdapter::new("not-a-postgres-url").await;
    assert!(result.is_err(), "expected error for malformed URL");
    let err = result.expect_err("error confirmed above");
    assert!(
        matches!(err, FraiseQLError::ConnectionPool { .. }),
        "expected ConnectionPool error for malformed URL, got: {err:?}"
    );
}

#[tokio::test]
async fn test_with_pool_size_malformed_url_returns_connection_pool_error() {
    let result = PostgresAdapter::with_pool_size("://bad-url", 1).await;
    assert!(result.is_err(), "expected error for bad URL");
    let err = result.expect_err("error confirmed above");
    assert!(
        matches!(err, FraiseQLError::ConnectionPool { .. }),
        "expected ConnectionPool error for bad URL with custom pool size, got: {err:?}"
    );
}
