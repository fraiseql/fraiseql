//! Tests for the query runner, co-located with `runners/query.rs`.

#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

use std::collections::HashMap;
use std::sync::Arc;

use chrono::Utc;
use indexmap::IndexMap;

use crate::{
    db::where_clause::WhereClause,
    db::types::JsonbValue,
    runtime::{Executor, RuntimeConfig},
    schema::{
        AutoParams, CompiledSchema, CursorType, FieldDefinition, FieldType, InjectedParamSource,
        QueryDefinition, TypeDefinition,
    },
    security::{DefaultRLSPolicy, SecurityContext},
};
use crate::runtime::executor::test_support::{
    CapturingMockAdapter, MockAdapter, mock_user_results, test_schema,
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

// ── mod session_variables: C-SV — set_session_variables called on reads ───

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

    /// Mock adapter that captures calls to `set_session_variables`.
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

        async fn set_session_variables(
            &self,
            variables: &[(&str, &str)],
        ) -> Result<()> {
            let mut guard = self.captured.lock().unwrap();
            for (k, v) in variables {
                guard.push(((*k).to_string(), (*v).to_string()));
            }
            Ok(())
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
        }
    }

    /// C-SV1: session variables are injected via `set_session_variables` before a read query.
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
            "set_session_variables must be called before a read query when session_variables are \
             configured"
        );
        assert!(
            pairs.iter().any(|(k, _)| k == "app.tenant_id"),
            "expected app.tenant_id in session variable pairs, got: {pairs:?}"
        );
    }

    /// C-SV2: no `set_session_variables` call when `session_variables` config is empty.
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
            "set_session_variables must not be called when no session_variables are configured"
        );
    }
}
