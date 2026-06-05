//! Tests for the query runner, co-located with `runners/query.rs`.

#![allow(clippy::unwrap_used, clippy::panic)] // Reason: test code, panics acceptable
use std::{collections::HashMap, sync::Arc};

use chrono::Utc;
use indexmap::IndexMap;

use crate::{
    db::{types::JsonbValue, where_clause::WhereClause},
    runtime::{
        Executor, RuntimeConfig,
        executor::test_support::{
            CapturingMockAdapter, MockAdapter, mock_user_results, test_schema,
        },
    },
    schema::{
        AutoParams, CompiledSchema, CursorType, FieldDefinition, FieldType, InjectedParamSource,
        QueryDefinition, TypeDefinition,
    },
    security::{DefaultRLSPolicy, SecurityContext},
};

// ── mod routing: per-view dispatch correctness ────────────────────────────

mod routing {
    use super::*;

    // R7: Per-view mock adapter routing verification ───────────────────────

    /// Multi-root queries dispatched to different views must return distinct results.
    /// This test would have silently passed before R7 because the old mock returned
    /// the same data for all views, masking routing bugs.
    #[tokio::test]
    async fn test_per_view_mock_returns_distinct_results() {
        let mut schema = CompiledSchema::new();
        schema.queries.push(QueryDefinition {
            name:                "users".to_string(),
            return_type:         "User".to_string(),
            returns_list:        true,
            nullable:            false,
            arguments:           Vec::new(),
            sql_source:          Some("v_user".to_string()),
            description:         None,
            auto_params:         AutoParams::default(),
            deprecation:         None,
            jsonb_column:        "data".to_string(),
            relay:               false,
            relay_cursor_column: None,
            relay_cursor_type:   CursorType::default(),
            inject_params:       IndexMap::default(),
            cache_ttl_seconds:   None,
            additional_views:    vec![],
            requires_role:       None,
            rest_path:           None,
            rest_method:         None,
            native_columns:      HashMap::new(),
        });

        let user_row = JsonbValue::new(serde_json::json!({"id": "1", "type": "user"}));
        let adapter = Arc::new(MockAdapter::new(vec![]).with_view("v_user", vec![user_row]));

        let executor = Executor::new(schema, adapter);
        let result = executor.execute("{ users { id type } }", None).await.unwrap();

        // v_user must return the user row, not the empty default.
        assert_eq!(result["data"]["users"][0]["type"], "user", "expected user row from v_user");
    }
}

// ── mod auto_params: has_where, has_limit, has_offset threading ──────────

mod auto_params {
    use super::*;

    fn schema_with_auto_params(auto_params: AutoParams) -> CompiledSchema {
        let mut schema = CompiledSchema::new();
        schema.queries.push(QueryDefinition {
            name: "users".to_string(),
            return_type: "User".to_string(),
            returns_list: true,
            nullable: false,
            arguments: Vec::new(),
            sql_source: Some("v_user".to_string()),
            description: None,
            auto_params,
            deprecation: None,
            jsonb_column: "data".to_string(),
            relay: false,
            relay_cursor_column: None,
            relay_cursor_type: CursorType::default(),
            inject_params: IndexMap::default(),
            cache_ttl_seconds: None,
            additional_views: vec![],
            requires_role: None,
            rest_path: None,
            rest_method: None,
            native_columns: HashMap::new(),
        });
        schema
    }

    #[tokio::test]
    async fn test_has_limit_threads_to_adapter() {
        let schema = schema_with_auto_params(AutoParams {
            has_limit:    true,
            has_offset:   false,
            has_where:    false,
            has_order_by: false,
        });
        let adapter = Arc::new(CapturingMockAdapter::new(mock_user_results()));
        let executor = Executor::new(schema, adapter.clone());

        let vars = serde_json::json!({"limit": 3});
        let _result = executor.execute("{ users { id name } }", Some(&vars)).await.unwrap();

        assert_eq!(adapter.captured_limit(), Some(3));
    }

    #[tokio::test]
    async fn test_limit_over_max_page_size_is_rejected() {
        // Default RuntimeConfig caps the top-level page size at 1000 (#421).
        let schema = schema_with_auto_params(AutoParams {
            has_limit:    true,
            has_offset:   false,
            has_where:    false,
            has_order_by: false,
        });
        let adapter = Arc::new(CapturingMockAdapter::new(mock_user_results()));
        let executor = Executor::new(schema, adapter.clone());

        let vars = serde_json::json!({"limit": 5000});
        let err = executor.execute("{ users { id name } }", Some(&vars)).await.unwrap_err();
        match err {
            crate::FraiseQLError::Validation { message, .. } => {
                assert!(message.contains("maximum page size"), "message was: {message}");
            },
            other => panic!("expected Validation error, got {other:?}"),
        }
        // Rejected before any SQL dispatch — the adapter was never queried.
        assert_eq!(adapter.captured_limit(), None);
    }

    #[tokio::test]
    async fn test_limit_at_max_page_size_is_allowed() {
        let schema = schema_with_auto_params(AutoParams {
            has_limit:    true,
            has_offset:   false,
            has_where:    false,
            has_order_by: false,
        });
        let adapter = Arc::new(CapturingMockAdapter::new(mock_user_results()));
        let executor = Executor::new(schema, adapter.clone());

        // Exactly at the default ceiling passes through unchanged.
        let vars = serde_json::json!({"limit": 1000});
        executor.execute("{ users { id name } }", Some(&vars)).await.unwrap();

        assert_eq!(adapter.captured_limit(), Some(1000));
    }

    #[tokio::test]
    async fn test_has_offset_threads_to_adapter() {
        let schema = schema_with_auto_params(AutoParams {
            has_limit:    false,
            has_offset:   true,
            has_where:    false,
            has_order_by: false,
        });
        let adapter = Arc::new(CapturingMockAdapter::new(mock_user_results()));
        let executor = Executor::new(schema, adapter.clone());

        let vars = serde_json::json!({"offset": 10});
        let _result = executor.execute("{ users { id name } }", Some(&vars)).await.unwrap();

        assert_eq!(adapter.captured_offset(), Some(10));
    }

    #[tokio::test]
    async fn test_has_where_threads_user_filter_to_adapter() {
        let schema = schema_with_auto_params(AutoParams {
            has_limit:    false,
            has_offset:   false,
            has_where:    true,
            has_order_by: false,
        });
        let adapter = Arc::new(CapturingMockAdapter::new(mock_user_results()));
        let executor = Executor::new(schema, adapter.clone());

        let vars = serde_json::json!({
            "where": {"name": {"eq": "Alice"}}
        });
        let _result = executor.execute("{ users { id name } }", Some(&vars)).await.unwrap();

        // The adapter should have received a WHERE clause
        let captured = adapter.captured_where();
        assert!(captured.is_some(), "expected WHERE clause to be passed to adapter");
    }

    #[tokio::test]
    async fn test_has_where_false_ignores_user_filter() {
        let schema = schema_with_auto_params(AutoParams {
            has_limit:    false,
            has_offset:   false,
            has_where:    false,
            has_order_by: false,
        });
        let adapter = Arc::new(CapturingMockAdapter::new(mock_user_results()));
        let executor = Executor::new(schema, adapter.clone());

        let vars = serde_json::json!({
            "where": {"name": {"eq": "Alice"}}
        });
        let _result = executor.execute("{ users { id name } }", Some(&vars)).await.unwrap();

        // WHERE clause should NOT be passed when has_where is false
        let captured = adapter.captured_where();
        assert!(captured.is_none(), "expected no WHERE clause when has_where is false");
    }

    #[tokio::test]
    async fn test_has_limit_and_offset_together() {
        let schema = schema_with_auto_params(AutoParams {
            has_limit:    true,
            has_offset:   true,
            has_where:    false,
            has_order_by: false,
        });
        let adapter = Arc::new(CapturingMockAdapter::new(mock_user_results()));
        let executor = Executor::new(schema, adapter.clone());

        let vars = serde_json::json!({"limit": 5, "offset": 20});
        let _result = executor.execute("{ users { id name } }", Some(&vars)).await.unwrap();

        assert_eq!(adapter.captured_limit(), Some(5));
        assert_eq!(adapter.captured_offset(), Some(20));
    }
}

// ── mod rls_composition: C13+C19 — WHERE composition through executor ────

mod rls_composition {
    use indexmap::IndexMap;

    use super::*;

    fn schema_with_inject_params(
        inject_params: IndexMap<String, InjectedParamSource>,
    ) -> CompiledSchema {
        let mut schema = CompiledSchema::new();
        schema.queries.push(QueryDefinition {
            name: "users".to_string(),
            return_type: "User".to_string(),
            returns_list: true,
            nullable: false,
            arguments: Vec::new(),
            sql_source: Some("v_user".to_string()),
            description: None,
            auto_params: AutoParams {
                has_where: true,
                ..AutoParams::default()
            },
            deprecation: None,
            jsonb_column: "data".to_string(),
            relay: false,
            relay_cursor_column: None,
            relay_cursor_type: CursorType::default(),
            inject_params,
            cache_ttl_seconds: None,
            additional_views: vec![],
            requires_role: None,
            rest_path: None,
            rest_method: None,
            native_columns: HashMap::new(),
        });
        schema
    }

    fn tenant_security_context() -> SecurityContext {
        SecurityContext {
            user_id:          "user-42".into(),
            roles:            vec!["viewer".to_string()],
            tenant_id:        Some("tenant-abc".into()),
            scopes:           vec!["read:User".to_string()],
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

    #[tokio::test]
    async fn test_rls_only_produces_where_clause() {
        let schema = schema_with_inject_params(IndexMap::new());
        let adapter = Arc::new(CapturingMockAdapter::new(mock_user_results()));
        let config = RuntimeConfig::default().with_rls_policy(Arc::new(DefaultRLSPolicy::new()));
        let executor = Executor::with_config(schema, adapter.clone(), config);

        let ctx = tenant_security_context();
        let _result = executor
            .execute_with_security("{ users { id name } }", None, &ctx)
            .await
            .unwrap();

        let captured = adapter.captured_where();
        assert!(captured.is_some(), "RLS policy should produce a WHERE clause for tenant user");
    }

    #[tokio::test]
    async fn test_inject_params_produces_where_clause() {
        let mut inject = IndexMap::new();
        inject.insert("tenant_id".to_string(), InjectedParamSource::Jwt("tenant_id".to_string()));
        let schema = schema_with_inject_params(inject);
        let adapter = Arc::new(CapturingMockAdapter::new(mock_user_results()));
        let executor = Executor::new(schema, adapter.clone());

        let ctx = tenant_security_context();
        let _result = executor
            .execute_with_security("{ users { id name } }", None, &ctx)
            .await
            .unwrap();

        let captured = adapter.captured_where();
        assert!(captured.is_some(), "inject_params should produce a WHERE clause");
    }

    /// C13: Verify RLS + `inject_params` compose into AND(rls, inject)
    #[tokio::test]
    async fn test_rls_and_inject_params_compose_into_and() {
        let mut inject = IndexMap::new();
        inject.insert("tenant_id".to_string(), InjectedParamSource::Jwt("tenant_id".to_string()));
        let schema = schema_with_inject_params(inject);
        let adapter = Arc::new(CapturingMockAdapter::new(mock_user_results()));
        let config = RuntimeConfig::default().with_rls_policy(Arc::new(DefaultRLSPolicy::new()));
        let executor = Executor::with_config(schema, adapter.clone(), config);

        let ctx = tenant_security_context();
        let _result = executor
            .execute_with_security("{ users { id name } }", None, &ctx)
            .await
            .unwrap();

        let captured = adapter.captured_where();
        assert!(captured.is_some(), "combined RLS + inject should produce a WHERE clause");
        // Should be an AND clause wrapping both conditions
        let where_clause = captured.unwrap();
        match &where_clause {
            WhereClause::And(clauses) => {
                assert!(
                    clauses.len() >= 2,
                    "expected at least 2 AND clauses (RLS + inject), got {}",
                    clauses.len()
                );
            },
            _ => panic!("expected AND composition, got: {where_clause:?}"),
        }
    }

    /// C19: Verify three-way composition: RLS + inject + user WHERE
    #[tokio::test]
    async fn test_three_way_where_composition_rls_inject_user() {
        let mut inject = IndexMap::new();
        inject.insert("tenant_id".to_string(), InjectedParamSource::Jwt("tenant_id".to_string()));
        let schema = schema_with_inject_params(inject);
        let adapter = Arc::new(CapturingMockAdapter::new(mock_user_results()));
        let config = RuntimeConfig::default().with_rls_policy(Arc::new(DefaultRLSPolicy::new()));
        let executor = Executor::with_config(schema, adapter.clone(), config);

        let ctx = tenant_security_context();
        let vars = serde_json::json!({
            "where": {"name": {"eq": "Alice"}}
        });
        let _result = executor
            .execute_with_security("{ users { id name } }", Some(&vars), &ctx)
            .await
            .unwrap();

        let captured = adapter.captured_where();
        assert!(captured.is_some(), "three-way composition should produce a WHERE clause");
        // Outermost should be AND(security_clause, user_where)
        let where_clause = captured.unwrap();
        match &where_clause {
            WhereClause::And(clauses) => {
                assert!(
                    clauses.len() >= 2,
                    "expected at least 2 top-level AND clauses, got {}",
                    clauses.len()
                );
                // The first clause should be the security AND(rls, inject)
                // The second clause should be the user WHERE
                // Together: AND(AND(rls, inject), user_where)
            },
            _ => panic!("expected AND composition, got: {where_clause:?}"),
        }
    }

    #[tokio::test]
    async fn test_inject_params_respects_native_columns() {
        let mut inject = IndexMap::new();
        inject.insert("tenant_id".to_string(), InjectedParamSource::Jwt("tenant_id".to_string()));
        let mut schema = CompiledSchema::new();
        let mut native_cols = HashMap::new();
        native_cols.insert("tenant_id".to_string(), "uuid".to_string());
        schema.queries.push(QueryDefinition {
            name:                "users".to_string(),
            return_type:         "User".to_string(),
            returns_list:        true,
            nullable:            false,
            arguments:           Vec::new(),
            sql_source:          Some("v_user".to_string()),
            description:         None,
            auto_params:         AutoParams {
                has_where: true,
                ..AutoParams::default()
            },
            deprecation:         None,
            jsonb_column:        "data".to_string(),
            relay:               false,
            relay_cursor_column: None,
            relay_cursor_type:   CursorType::default(),
            inject_params:       inject,
            cache_ttl_seconds:   None,
            additional_views:    vec![],
            requires_role:       None,
            rest_path:           None,
            rest_method:         None,
            native_columns:      native_cols,
        });
        schema.types.push({
            let mut t = TypeDefinition::new("User", "v_user");
            t.fields = vec![
                FieldDefinition::new("id", FieldType::Int),
                FieldDefinition::new("name", FieldType::String),
            ];
            t
        });

        let adapter = Arc::new(CapturingMockAdapter::new(mock_user_results()));
        let executor = Executor::new(schema, adapter.clone());

        let ctx = tenant_security_context();
        let _result = executor
            .execute_with_security("{ users { id name } }", None, &ctx)
            .await
            .unwrap();

        let captured = adapter.captured_where();
        assert!(captured.is_some(), "inject with native_columns should produce WHERE");
        match captured.unwrap() {
            WhereClause::NativeField {
                column, pg_cast, ..
            } => {
                assert_eq!(column, "tenant_id");
                assert_eq!(pg_cast, "uuid");
            },
            other => panic!("expected NativeField for native_columns inject, got: {other:?}"),
        }
    }
}

// ── mod session_variables: C-SV — session variables passed into reads ─────

mod session_variables {
    use async_trait::async_trait;

    use super::*;
    use crate::{
        db::{
            traits::DatabaseAdapter,
            types::{DatabaseType, JsonbValue, PoolMetrics, sql_hints::OrderByClause},
            where_clause::WhereClause,
        },
        error::Result,
        schema::{SessionVariableMapping, SessionVariableSource, SessionVariablesConfig},
    };

    /// Mock adapter that captures the session variables passed into the
    /// connection-affine `*_with_session` read methods (#329).
    struct SessionVarCapturingAdapter {
        mock_results: Vec<JsonbValue>,
        captured:     std::sync::Mutex<Vec<(String, String)>>,
    }

    impl SessionVarCapturingAdapter {
        fn new(mock_results: Vec<JsonbValue>) -> Self {
            Self {
                mock_results,
                captured: std::sync::Mutex::new(Vec::new()),
            }
        }

        fn captured_pairs(&self) -> Vec<(String, String)> {
            self.captured.lock().unwrap().clone()
        }
    }

    // Reason: DatabaseAdapter is defined with #[async_trait]; all implementations must match
    // async_trait: dyn-dispatch required; remove when RTN + Send is stable (RFC 3425)
    #[async_trait]
    impl DatabaseAdapter for SessionVarCapturingAdapter {
        async fn execute_with_projection(
            &self,
            _view: &str,
            _projection: Option<&crate::schema::SqlProjectionHint>,
            _where_clause: Option<&WhereClause>,
            _limit: Option<u32>,
            _offset: Option<u32>,
            _order_by: Option<&[OrderByClause]>,
        ) -> Result<Vec<JsonbValue>> {
            Ok(self.mock_results.clone())
        }

        async fn execute_where_query(
            &self,
            _view: &str,
            _where_clause: Option<&WhereClause>,
            _limit: Option<u32>,
            _offset: Option<u32>,
            _order_by: Option<&[OrderByClause]>,
        ) -> Result<Vec<JsonbValue>> {
            Ok(self.mock_results.clone())
        }

        async fn execute_with_projection_arc_with_session(
            &self,
            _request: &crate::db::ProjectionRequest<'_>,
            session_vars: &[(&str, &str)],
        ) -> Result<std::sync::Arc<Vec<JsonbValue>>> {
            let mut guard = self.captured.lock().unwrap();
            for (k, v) in session_vars {
                guard.push(((*k).to_string(), (*v).to_string()));
            }
            Ok(std::sync::Arc::new(self.mock_results.clone()))
        }

        async fn execute_where_query_arc_with_session(
            &self,
            _view: &str,
            _where_clause: Option<&WhereClause>,
            _limit: Option<u32>,
            _offset: Option<u32>,
            _order_by: Option<&[OrderByClause]>,
            session_vars: &[(&str, &str)],
        ) -> Result<std::sync::Arc<Vec<JsonbValue>>> {
            let mut guard = self.captured.lock().unwrap();
            for (k, v) in session_vars {
                guard.push(((*k).to_string(), (*v).to_string()));
            }
            Ok(std::sync::Arc::new(self.mock_results.clone()))
        }

        async fn health_check(&self) -> Result<()> {
            Ok(())
        }

        fn database_type(&self) -> DatabaseType {
            DatabaseType::PostgreSQL
        }

        fn pool_metrics(&self) -> PoolMetrics {
            PoolMetrics {
                total_connections:  1,
                active_connections: 0,
                idle_connections:   1,
                waiting_requests:   0,
            }
        }

        async fn execute_raw_query(
            &self,
            _sql: &str,
        ) -> Result<Vec<std::collections::HashMap<String, serde_json::Value>>> {
            Ok(vec![])
        }

        async fn execute_parameterized_aggregate(
            &self,
            _sql: &str,
            _params: &[serde_json::Value],
        ) -> Result<Vec<std::collections::HashMap<String, serde_json::Value>>> {
            Ok(vec![])
        }

        async fn execute_function_call(
            &self,
            _function_name: &str,
            _args: &[serde_json::Value],
        ) -> Result<Vec<std::collections::HashMap<String, serde_json::Value>>> {
            Ok(vec![])
        }
    }

    fn schema_with_session_vars() -> CompiledSchema {
        let mut schema = test_schema();
        schema.session_variables = SessionVariablesConfig {
            variables:         vec![SessionVariableMapping {
                name:   "app.tenant_id".to_string(),
                source: SessionVariableSource::Jwt {
                    claim: "tenant_id".to_string(),
                },
            }],
            inject_started_at: false,
        };
        schema
    }

    fn security_ctx_with_tenant() -> SecurityContext {
        SecurityContext {
            user_id:          "user-1".into(),
            roles:            vec![],
            tenant_id:        Some("tenant-abc".into()),
            scopes:           vec![],
            attributes:       HashMap::default(),
            request_id:       "req-sv".to_string(),
            ip_address:       None,
            expires_at:       Utc::now() + chrono::Duration::hours(1),
            authenticated_at: Utc::now(),
            issuer:           None,
            audience:         None,
            email:            None,
            display_name:     None,
        }
    }

    /// C-SV1: session variables are passed into the connection-affine read
    /// method when configured.
    #[tokio::test]
    async fn test_session_variables_injected_on_read_query() {
        let schema = schema_with_session_vars();
        let adapter = Arc::new(SessionVarCapturingAdapter::new(mock_user_results()));
        let executor = Executor::new(schema, adapter.clone());

        let ctx = security_ctx_with_tenant();
        executor
            .execute_with_security("{ users { id name } }", None, &ctx)
            .await
            .unwrap();

        let pairs = adapter.captured_pairs();
        assert!(
            !pairs.is_empty(),
            "session variables must be passed into the read method when session_variables are \
             configured"
        );
        assert!(
            pairs.iter().any(|(k, _)| k == "app.tenant_id"),
            "expected app.tenant_id in session variable pairs, got: {pairs:?}"
        );
    }

    /// C-SV2: no session variables passed when `session_variables` config is empty.
    #[tokio::test]
    async fn test_no_session_variables_injected_when_config_empty() {
        let schema = test_schema(); // session_variables defaults to empty
        let adapter = Arc::new(SessionVarCapturingAdapter::new(mock_user_results()));
        let executor = Executor::new(schema, adapter.clone());

        let ctx = security_ctx_with_tenant();
        executor
            .execute_with_security("{ users { id name } }", None, &ctx)
            .await
            .unwrap();

        assert!(
            adapter.captured_pairs().is_empty(),
            "no session variables must be passed when no session_variables are configured"
        );
    }
}

// ---------------------------------------------------------------------------
// Inline tests from query.rs (projection_reduction, pg_type_to_cast)
// ---------------------------------------------------------------------------

mod pg_type_cast_tests {
    use super::super::*;
    use crate::graphql::FieldSelection;

    // -------------------------------------------------------------------------
    // Helpers
    // -------------------------------------------------------------------------

    fn leaf(name: &str) -> FieldSelection {
        FieldSelection {
            name:          name.to_string(),
            alias:         None,
            arguments:     vec![],
            nested_fields: vec![],
            directives:    vec![],
        }
    }

    fn fragment(name: &str, nested: Vec<FieldSelection>) -> FieldSelection {
        FieldSelection {
            name:          name.to_string(),
            alias:         None,
            arguments:     vec![],
            nested_fields: nested,
            directives:    vec![],
        }
    }

    // =========================================================================
    // compute_projection_reduction
    // =========================================================================

    #[test]
    fn projection_reduction_zero_fields_is_clamped_to_90() {
        // 0 fields requested → saved = 20 → 100% → clamped to 90
        assert_eq!(compute_projection_reduction(0), 90);
    }

    #[test]
    fn projection_reduction_all_fields_is_clamped_to_10() {
        // 20 fields (= baseline) → saved = 0 → 0% → clamped to 10
        assert_eq!(compute_projection_reduction(20), 10);
    }

    #[test]
    fn projection_reduction_above_baseline_clamps_to_10() {
        // 50 fields > 20 baseline → same as 20 → clamped to 10
        assert_eq!(compute_projection_reduction(50), 10);
    }

    #[test]
    fn projection_reduction_10_fields_is_50_percent() {
        // 10 requested → saved = 10 → 10/20 * 100 = 50 → within [10, 90]
        assert_eq!(compute_projection_reduction(10), 50);
    }

    #[test]
    fn projection_reduction_1_field_is_high() {
        // 1 requested → saved = 19 → 95% → clamped to 90
        assert_eq!(compute_projection_reduction(1), 90);
    }

    #[test]
    fn projection_reduction_result_always_in_clamp_range() {
        for n in 0_usize..=30 {
            let r = compute_projection_reduction(n);
            assert!((10..=90).contains(&r), "out of [10,90] for n={n}: got {r}");
        }
    }

    // =========================================================================
    // selections_contain_field
    // =========================================================================

    #[test]
    fn empty_selections_returns_false() {
        assert!(!selections_contain_field(&[], "totalCount"));
    }

    #[test]
    fn direct_match_returns_true() {
        let sels = vec![leaf("edges"), leaf("totalCount"), leaf("pageInfo")];
        assert!(selections_contain_field(&sels, "totalCount"));
    }

    #[test]
    fn absent_field_returns_false() {
        let sels = vec![leaf("edges"), leaf("pageInfo")];
        assert!(!selections_contain_field(&sels, "totalCount"));
    }

    #[test]
    fn inline_fragment_nested_match_returns_true() {
        // "...on UserConnection" wrapping totalCount
        let inline = fragment("...on UserConnection", vec![leaf("totalCount"), leaf("edges")]);
        let sels = vec![inline];
        assert!(selections_contain_field(&sels, "totalCount"));
    }

    #[test]
    fn inline_fragment_does_not_spuriously_match_fragment_name() {
        // The fragment entry (name "...on Foo") only matches a field named exactly "...on Foo"
        // when searched directly; it should NOT match an unrelated field name.
        let inline = fragment("...on Foo", vec![leaf("id")]);
        let sels = vec![inline];
        assert!(!selections_contain_field(&sels, "totalCount"));
        // "id" is nested inside the fragment and should be found via recursion
        assert!(selections_contain_field(&sels, "id"));
    }

    #[test]
    fn field_not_in_fragment_returns_false() {
        let inline = fragment("...on UserConnection", vec![leaf("edges"), leaf("pageInfo")]);
        let sels = vec![inline];
        assert!(!selections_contain_field(&sels, "totalCount"));
    }

    #[test]
    fn non_fragment_nested_field_not_searched() {
        // Only entries whose name starts with "..." trigger recursion.
        // A plain field's nested_fields should NOT be recursed into.
        let nested_count = fragment("edges", vec![leaf("totalCount")]);
        let sels = vec![nested_count];
        // "edges" doesn't start with "..." — nested fields not searched
        assert!(!selections_contain_field(&sels, "totalCount"));
    }

    #[test]
    fn multiple_fragments_any_can_match() {
        let frag1 = fragment("...on TypeA", vec![leaf("id")]);
        let frag2 = fragment("...on TypeB", vec![leaf("totalCount")]);
        let sels = vec![frag1, frag2];
        assert!(selections_contain_field(&sels, "totalCount"));
        assert!(selections_contain_field(&sels, "id"));
        assert!(!selections_contain_field(&sels, "name"));
    }

    #[test]
    fn mixed_direct_and_fragment_selections() {
        let inline = fragment("...on Connection", vec![leaf("pageInfo")]);
        let sels = vec![leaf("edges"), inline, leaf("metadata")];
        assert!(selections_contain_field(&sels, "edges"));
        assert!(selections_contain_field(&sels, "pageInfo"));
        assert!(selections_contain_field(&sels, "metadata"));
        assert!(!selections_contain_field(&sels, "cursor"));
    }

    // =========================================================================
    // combine_explicit_arg_where
    // =========================================================================

    use crate::schema::{ArgumentDefinition, FieldType};

    fn make_arg(name: &str) -> ArgumentDefinition {
        ArgumentDefinition::new(name, FieldType::Id)
    }

    #[test]
    fn no_explicit_args_returns_existing() {
        let existing = Some(WhereClause::Field {
            path:     vec!["rls".into()],
            operator: WhereOperator::Eq,
            value:    serde_json::json!("x"),
        });
        let result = combine_explicit_arg_where(
            existing.clone(),
            &[],
            &std::collections::HashMap::new(),
            &std::collections::HashMap::new(),
        );
        assert_eq!(result, existing);
    }

    #[test]
    fn explicit_id_arg_produces_where_clause() {
        let args = vec![make_arg("id")];
        let mut provided = std::collections::HashMap::new();
        provided.insert("id".into(), serde_json::json!("uuid-123"));

        let result =
            combine_explicit_arg_where(None, &args, &provided, &std::collections::HashMap::new());
        assert!(result.is_some(), "explicit id arg should produce a WHERE clause");
        match result.expect("just asserted Some") {
            WhereClause::Field {
                path,
                operator,
                value,
            } => {
                assert_eq!(path, vec!["id".to_string()]);
                assert_eq!(operator, WhereOperator::Eq);
                assert_eq!(value, serde_json::json!("uuid-123"));
            },
            other => panic!("expected Field, got {other:?}"),
        }
    }

    #[test]
    fn auto_param_names_are_skipped() {
        let args = vec![
            make_arg("where"),
            make_arg("limit"),
            make_arg("offset"),
            make_arg("orderBy"),
            make_arg("first"),
            make_arg("last"),
            make_arg("after"),
            make_arg("before"),
            make_arg("id"),
        ];
        let mut provided = std::collections::HashMap::new();
        for name in &[
            "where", "limit", "offset", "orderBy", "first", "last", "after", "before", "id",
        ] {
            provided.insert((*name).to_string(), serde_json::json!("value"));
        }

        let result =
            combine_explicit_arg_where(None, &args, &provided, &std::collections::HashMap::new());
        // Only "id" should produce a WHERE — all auto-param names are skipped
        match result.expect("id arg should produce WHERE") {
            WhereClause::Field { path, .. } => {
                assert_eq!(path, vec!["id".to_string()]);
            },
            other => panic!("expected single Field for 'id', got {other:?}"),
        }
    }

    #[test]
    fn explicit_args_combined_with_existing_where() {
        let existing = WhereClause::Field {
            path:     vec!["rls_tenant".into()],
            operator: WhereOperator::Eq,
            value:    serde_json::json!("tenant-1"),
        };
        let args = vec![make_arg("id")];
        let mut provided = std::collections::HashMap::new();
        provided.insert("id".into(), serde_json::json!("uuid-456"));

        let result = combine_explicit_arg_where(
            Some(existing),
            &args,
            &provided,
            &std::collections::HashMap::new(),
        );
        match result.expect("should produce combined WHERE") {
            WhereClause::And(conditions) => {
                assert_eq!(conditions.len(), 2, "should AND existing + explicit");
            },
            other => panic!("expected And, got {other:?}"),
        }
    }

    #[test]
    fn unprovided_explicit_arg_is_ignored() {
        let args = vec![make_arg("id"), make_arg("slug")];
        let mut provided = std::collections::HashMap::new();
        // Only provide "id", not "slug"
        provided.insert("id".into(), serde_json::json!("uuid-789"));

        let result =
            combine_explicit_arg_where(None, &args, &provided, &std::collections::HashMap::new());
        match result.expect("id arg should produce WHERE") {
            WhereClause::Field { path, .. } => {
                assert_eq!(path, vec!["id".to_string()]);
            },
            other => panic!("expected single Field for 'id', got {other:?}"),
        }
    }

    // =========================================================================
    // pg_type_to_cast — returns canonical type names passed to SqlDialect::cast_native_param
    // =========================================================================

    #[test]
    fn uuid_normalises_to_canonical_type_name() {
        assert_eq!(pg_type_to_cast("uuid"), "uuid");
        assert_eq!(pg_type_to_cast("UUID"), "uuid");
    }

    #[test]
    fn integer_types_normalise_to_canonical_names() {
        assert_eq!(pg_type_to_cast("integer"), "int4");
        assert_eq!(pg_type_to_cast("int4"), "int4");
        assert_eq!(pg_type_to_cast("bigint"), "int8");
        assert_eq!(pg_type_to_cast("int8"), "int8");
        assert_eq!(pg_type_to_cast("smallint"), "int2");
        assert_eq!(pg_type_to_cast("int2"), "int2");
    }

    #[test]
    fn float_and_numeric_types_normalise_to_canonical_names() {
        assert_eq!(pg_type_to_cast("numeric"), "numeric");
        assert_eq!(pg_type_to_cast("decimal"), "numeric");
        assert_eq!(pg_type_to_cast("double precision"), "float8");
        assert_eq!(pg_type_to_cast("float8"), "float8");
        assert_eq!(pg_type_to_cast("real"), "float4");
        assert_eq!(pg_type_to_cast("float4"), "float4");
    }

    #[test]
    fn date_and_time_types_normalise_to_canonical_names() {
        assert_eq!(pg_type_to_cast("timestamp"), "timestamp");
        assert_eq!(pg_type_to_cast("timestamp without time zone"), "timestamp");
        assert_eq!(pg_type_to_cast("timestamptz"), "timestamptz");
        assert_eq!(pg_type_to_cast("timestamp with time zone"), "timestamptz");
        assert_eq!(pg_type_to_cast("date"), "date");
        assert_eq!(pg_type_to_cast("time"), "time");
        assert_eq!(pg_type_to_cast("time without time zone"), "time");
    }

    #[test]
    fn bool_normalises_to_canonical_name() {
        assert_eq!(pg_type_to_cast("boolean"), "bool");
        assert_eq!(pg_type_to_cast("bool"), "bool");
    }

    #[test]
    fn text_types_produce_empty_hint_meaning_no_cast() {
        assert_eq!(pg_type_to_cast("text"), "");
        assert_eq!(pg_type_to_cast("varchar"), "");
        assert_eq!(pg_type_to_cast("unknown_type"), "");
    }
}
