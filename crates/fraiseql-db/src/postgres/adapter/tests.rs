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
    let cfg = PoolPrewarmConfig {
        min_size:     5,
        max_size:     20,
        timeout_secs: Some(30),
    };
    assert_eq!(cfg.min_size, 5);
    assert_eq!(cfg.max_size, 20);
    assert_eq!(cfg.timeout_secs, Some(30));
}

#[test]
fn pool_prewarm_config_no_timeout_is_none() {
    let cfg = PoolPrewarmConfig {
        min_size:     0,
        max_size:     10,
        timeout_secs: None,
    };
    assert!(cfg.timeout_secs.is_none());
}

#[test]
fn pool_prewarm_config_min_zero_is_valid() {
    let cfg = PoolPrewarmConfig {
        min_size:     0,
        max_size:     5,
        timeout_secs: None,
    };
    assert_eq!(cfg.min_size, 0);
    assert_eq!(cfg.max_size, 5);
}

#[test]
fn pool_prewarm_config_min_equals_max_is_valid() {
    let cfg = PoolPrewarmConfig {
        min_size:     10,
        max_size:     10,
        timeout_secs: Some(60),
    };
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

// ── build_changelog_cte_sql (Change Spine in-txn outbox, phase-02) ──────────

#[test]
fn changelog_cte_runs_the_function_once_and_inserts_the_outbox_row() {
    let sql = super::database::build_changelog_cte_sql(r#""app"."fn_create_user""#, 2);

    // The function is materialised so a volatile mutation runs EXACTLY once even
    // though both the INSERT CTE and the primary SELECT read it.
    assert!(
        sql.contains(r#"WITH r AS MATERIALIZED (SELECT * FROM "app"."fn_create_user"($1, $2))"#),
        "function call must be a MATERIALIZED CTE: {sql}"
    );
    // One outbox row, into the framework-owned contract table.
    assert!(
        sql.contains("INSERT INTO core.tb_entity_change_log"),
        "must INSERT into the contract table: {sql}"
    );
    // The primary query returns the function's row unchanged to the caller.
    assert!(
        sql.trim_end().ends_with("SELECT * FROM r"),
        "must return the function row: {sql}"
    );
}

#[test]
fn changelog_cte_threads_object_type_fallback_and_modification_type_after_args() {
    // 2 function args => object_type fallback is $3, modification_type is $4.
    let sql = super::database::build_changelog_cte_sql(r#""fn_x""#, 2);
    assert!(
        sql.contains("COALESCE(r.entity_type, $3)"),
        "object_type falls back to $n+1 when entity_type is NULL: {sql}"
    );
    // The verb param ($4) is selected straight into modification_type.
    assert!(sql.contains("$4,"), "modification_type verb is $n+2: {sql}");

    // 0 args => fallback is $1, verb is $2.
    let zero = super::database::build_changelog_cte_sql(r#""fn_y""#, 0);
    assert!(
        zero.contains(r#"SELECT * FROM "fn_y"()"#),
        "no-arg call has empty parens: {zero}"
    );
    assert!(
        zero.contains("COALESCE(r.entity_type, $1)"),
        "fallback is $1 with no args: {zero}"
    );
    assert!(zero.contains("$2,"), "verb is $2 with no args: {zero}");
}

#[test]
fn changelog_cte_only_logs_effective_changes_and_stamps_the_duration_marker() {
    let sql = super::database::build_changelog_cte_sql(r#""fn_z""#, 1);
    // Only an effective change (succeeded AND state_changed) is logged — no-ops
    // and business-logic failures must NOT fan out to the spine.
    assert!(
        sql.contains("WHERE r.succeeded AND r.state_changed"),
        "must gate on an effective change: {sql}"
    );
    // duration_ms uses the canonical wall-clock computation, not the
    // EXTRACT(MILLISECONDS) trap, on the DB clock.
    assert!(sql.contains("EXTRACT(EPOCH"), "canonical duration computation: {sql}");
    assert!(
        !sql.to_uppercase().contains("MILLISECONDS"),
        "must avoid the MILLISECONDS trap: {sql}"
    );
    assert!(
        sql.contains("current_setting('fraiseql.started_at')"),
        "reads started_at from the txn-local GUC: {sql}"
    );
    // The data-quality marker lets #392 refuse to mix pre/post-fix rows.
    assert!(
        sql.contains("'duration_calc_version', 2::int"),
        "stamps the duration_calc_version marker: {sql}"
    );
}

#[test]
fn changelog_cte_maps_mutation_response_columns_to_contract_columns() {
    let sql = super::database::build_changelog_cte_sql(r#""fn_w""#, 0);
    // The changed-entity payload columns come straight off the function's
    // app.mutation_response row (object_id<-entity_id, object_data<-entity, …).
    for needle in [
        "(object_type, modification_type, object_id, object_data, updated_fields, cascade, \
         started_at, duration_ms, extra_metadata)",
        "r.entity_id, r.entity, r.updated_fields, r.cascade",
    ] {
        assert!(sql.contains(needle), "expected `{needle}` in: {sql}");
    }
}
