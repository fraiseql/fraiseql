//! Tests for the query runner, co-located with `runners/query.rs`.

#![allow(clippy::unwrap_used, clippy::panic)] // Reason: test code, panics acceptable
use std::{collections::HashMap, sync::Arc};

use chrono::Utc;
use indexmap::IndexMap;

use crate::{
    backend::{types::JsonbValue, where_clause::WhereClause},
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

// ── mod sourceless: a query with no SQL source is refused, never served ───

mod sourceless {
    use super::*;

    /// #687: an embedded value object suppresses its synthesized source, so a query
    /// rooted at one compiles to a `QueryDefinition` with `sql_source: None` (pinned on
    /// the compile side by `embedded_value_object_cascade_e2e` in fraiseql-cli). The
    /// runner must refuse such a query loudly — a `Validation` error naming the missing
    /// source — never answer with rows or an empty result.
    #[tokio::test]
    async fn query_with_no_sql_source_is_refused_loudly() {
        let mut schema = CompiledSchema::new();
        schema.queries.push(QueryDefinition {
            function: None,

            requires_actor:      Vec::new(),
            returns_count:       false,
            name:                "money".to_string(),
            return_type:         "Money".to_string(),
            returns_list:        false,
            nullable:            true,
            arguments:           Vec::new(),
            sql_source:          None,
            description:         None,
            auto_params:         AutoParams::default(),
            deprecation:         None,
            jsonb_column:        "data".to_string(),
            relay:               false,
            relay_cursor_column: None,
            relay_cursor_type:   CursorType::default(),
            inject_params:       IndexMap::default(),
            read_routing:        crate::backend::types::ReadRouting::default(),
            cache_ttl_seconds:   None,
            additional_views:    vec![],
            requires_role:       None,
            rest_path:           None,
            rest_method:         None,
            rest_stream:         false,
            native_columns:      HashMap::new(),
            pagination_order:    None,
        });

        // The adapter has rows to give: if the runner dispatched anyway, the query
        // would succeed and `unwrap_err` below would catch the regression.
        let adapter = Arc::new(MockAdapter::new(mock_user_results()));
        let executor = Executor::new(schema, adapter);

        let err = executor.execute("{ money { amount } }", None).await.unwrap_err();
        match err {
            crate::FraiseQLError::Validation { message, .. } => {
                assert!(message.contains("no SQL source"), "message was: {message}");
            },
            other => panic!("expected Validation error, got {other:?}"),
        }
    }
}

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
            function: None,

            requires_actor:      Vec::new(),
            returns_count:       false,
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
            read_routing:        crate::backend::types::ReadRouting::default(),
            cache_ttl_seconds:   None,
            additional_views:    vec![],
            requires_role:       None,
            rest_path:           None,
            rest_method:         None,
            rest_stream:         false,
            native_columns:      HashMap::new(),
            pagination_order:    None,
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
            function: None,

            requires_actor: Vec::new(),
            returns_count: false,
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
            read_routing: crate::backend::types::ReadRouting::default(),
            cache_ttl_seconds: None,
            additional_views: vec![],
            requires_role: None,
            rest_path: None,
            rest_method: None,
            rest_stream: false,
            native_columns: HashMap::new(),
            pagination_order: None,
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

    /// #1283: a filter a query does not accept is **refused**, and the read does not run.
    ///
    /// This case used to assert the opposite — that the adapter received no WHERE clause
    /// — which is what "the whole relation, under a 200" looks like from inside. The
    /// caller cannot tell that answer from a filter that matched every row, and on the
    /// REST surface, where the filter arrives as `?name=Alice` and is validated against
    /// the type before it is discarded, it cannot tell it from a filter that worked.
    ///
    /// The variables spelling is the reachable one on this path: a bare `where` variable
    /// becomes `arguments["where"]` (`match_query` seeds the argument map from the
    /// variables), while the inline spelling — `users(where: …)` — is already refused one
    /// step earlier by `validate_argument_names` (#1154), because `graphql_arguments`
    /// omits the argument this flag turns off.
    #[tokio::test]
    async fn test_has_where_false_refuses_user_filter() {
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
        let err = executor
            .execute("{ users { id name } }", Some(&vars))
            .await
            .expect_err("a filter this query cannot apply is refused");
        let message = err.to_string();
        assert!(message.contains("users"), "the refusal names the query: {message}");
        assert!(
            message.contains("where_clause = false"),
            "and the setting that produced it: {message}"
        );

        assert!(adapter.captured_where().is_none(), "and the read never reached the database");
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
            function: None,

            requires_actor: Vec::new(),
            returns_count: false,
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
            read_routing: crate::backend::types::ReadRouting::default(),
            cache_ttl_seconds: None,
            additional_views: vec![],
            requires_role: None,
            rest_path: None,
            rest_method: None,
            rest_stream: false,
            native_columns: HashMap::new(),
            pagination_order: None,
        });
        schema
    }

    /// The same schema, plus a `User` type that actually declares fields — so
    /// `where_field_types` can adjudicate. Without a type the level is `None`
    /// and every key fails open (#939), which would make a spelling test pass
    /// for the wrong reason.
    fn schema_with_inject_params_and_user_type(
        inject_params: IndexMap<String, InjectedParamSource>,
    ) -> CompiledSchema {
        use crate::schema::{FieldDefinition, FieldType, TypeDefinition};

        let mut schema = schema_with_inject_params(inject_params);
        let mut user = TypeDefinition::new("User", "v_user");
        user.fields = vec![
            FieldDefinition::new("id", FieldType::parse("ID")),
            FieldDefinition::new("createdAt", FieldType::parse("DateTime")),
        ];
        schema.types.push(user);
        schema
    }

    /// **Tenant isolation is not a client-input concern — pinned.**
    ///
    /// Tightening `where` to the declared spelling binds at the client-input
    /// boundary only. Injected params never cross it: `inject_param_where_clause`
    /// builds a `WhereClause` value straight from the configured column, so the
    /// tenant predicate cannot be refused by a rule about how a *client* spelled
    /// a key — even though the injected column here is `tenant_id`, precisely the
    /// `snake_case` shape the rule now rejects from a client.
    ///
    /// Both halves matter. If the rule ever reached injected params, the tenant
    /// condition would vanish from the composed clause and rows would leak; this
    /// asserts it is still there. And a refused client key must fail **closed** —
    /// no query reaching the adapter — rather than fall back to an unfiltered read.
    #[tokio::test]
    async fn tenancy_survives_the_where_spelling_rule_and_a_refusal_fails_closed() {
        let mut inject = IndexMap::new();
        inject.insert("tenant_id".to_string(), InjectedParamSource::Jwt("tenant_id".to_string()));

        // 1. The declared spelling composes with the injected tenant predicate.
        let adapter = Arc::new(CapturingMockAdapter::new(mock_user_results()));
        let executor = Executor::with_config(
            schema_with_inject_params_and_user_type(inject.clone()),
            adapter.clone(),
            RuntimeConfig::default(),
        );
        let ctx = tenant_security_context();
        let vars = serde_json::json!({ "where": { "createdAt": { "eq": "2026-01-01" } } });
        executor
            .execute_with_security("{ users { id } }", Some(&vars), &ctx)
            .await
            .expect("`createdAt` is the declared spelling and must execute");

        let composed =
            adapter.captured_where().expect("a filtered, tenant-scoped read has a WHERE");
        let rendered = format!("{composed:?}");
        assert!(
            rendered.contains("tenant_id"),
            "the injected tenant predicate must survive the spelling rule: {rendered}"
        );
        assert!(
            rendered.contains("created_at"),
            "and the user filter must still lower to its storage path: {rendered}"
        );

        // 2. The storage spelling is refused, and nothing reaches the database.
        let adapter2 = Arc::new(CapturingMockAdapter::new(mock_user_results()));
        let executor2 = Executor::with_config(
            schema_with_inject_params_and_user_type(inject),
            adapter2.clone(),
            RuntimeConfig::default(),
        );
        let bad = serde_json::json!({ "where": { "created_at": { "eq": "2026-01-01" } } });
        let err = executor2
            .execute_with_security("{ users { id } }", Some(&bad), &ctx)
            .await
            .expect_err("the storage spelling is not part of the published filter input");
        assert!(
            err.to_string().contains("createdAt"),
            "the error names the spelling that works: {err}"
        );
        assert!(
            adapter2.captured_where().is_none(),
            "a refused filter must fail closed, never fall through to an unfiltered read"
        );
    }

    fn tenant_security_context() -> SecurityContext {
        SecurityContext {
            user_id:          "user-42".into(),
            roles:            vec!["viewer".to_string()],
            tenant_id:        Some("tenant-abc".into()),
            scopes:           vec!["read:User".to_string()],
            attributes:       HashMap::from([(
                "tenant_id".to_string(),
                serde_json::json!("tenant-abc"),
            )]),
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
            function: None,

            requires_actor:      Vec::new(),
            returns_count:       false,
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
            read_routing:        crate::backend::types::ReadRouting::default(),
            cache_ttl_seconds:   None,
            additional_views:    vec![],
            requires_role:       None,
            rest_path:           None,
            rest_method:         None,
            rest_stream:         false,
            native_columns:      native_cols,
            pagination_order:    None,
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
        backend::{
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
        // A test double: the session variables a read carries are accepted (#1115).
        fn applies_session_variables(&self) -> bool {
            true
        }

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
            _request: &crate::backend::ProjectionRequest<'_>,
            session_vars: &[(&str, &str)],
            _routing: crate::backend::types::ReadRouting,
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
            _routing: crate::backend::types::ReadRouting,
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
    }

    // The writes this double answers, called by its `Writer` impl below.
    impl SessionVarCapturingAdapter {
        async fn execute_function_call(
            &self,
            _function_name: &str,
            _args: &[serde_json::Value],
        ) -> Result<Vec<std::collections::HashMap<String, serde_json::Value>>> {
            Ok(vec![])
        }
    }

    // async_trait: dyn-dispatch required; remove when RTN + Send is stable (RFC 3425)
    #[async_trait::async_trait]
    impl crate::backend::traits::Writer for SessionVarCapturingAdapter {
        async fn execute_write(
            &self,
            request: &crate::backend::traits::WriteRequest<'_>,
            gate: crate::backend::traits::MutationRowGate<'_>,
        ) -> std::result::Result<
            Vec<std::collections::HashMap<String, serde_json::Value>>,
            crate::error::FraiseQLError,
        > {
            {
                let rows = self.execute_function_call(request.function, request.args).await?;
                gate(&rows)?;
                Ok(rows)
            }
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
            attributes:       HashMap::from([(
                "tenant_id".to_string(),
                serde_json::json!("tenant-abc"),
            )]),
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
        let executor = Executor::read_only(schema, adapter.clone());

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

    /// `fraiseql.started_at` times a **mutation** (the change log's duration); a read
    /// never consults it. Injecting it on reads set a per-request clock directive on
    /// every authenticated read, which is what made the row cache skip all of them in a
    /// deployment that declared `[session_variables]` (#1373).
    #[tokio::test]
    async fn a_read_does_not_carry_the_mutation_timestamp() {
        let mut schema = test_schema();
        // A real mapping, so the read resolves its session variables: what is pinned is
        // that the resolution leaves the timestamp out, not that it never ran.
        schema.session_variables = SessionVariablesConfig {
            variables:         vec![SessionVariableMapping {
                name:   "app.tenant_id".to_string(),
                source: SessionVariableSource::Jwt {
                    claim: "tenant_id".to_string(),
                },
            }],
            inject_started_at: true,
        };
        let adapter = Arc::new(SessionVarCapturingAdapter::new(mock_user_results()));
        let executor = Executor::read_only(schema, adapter.clone());

        executor
            .execute_with_security("{ users { id name } }", None, &security_ctx_with_tenant())
            .await
            .unwrap();

        let pairs = adapter.captured_pairs();
        assert!(
            pairs.iter().any(|(k, _)| k == "app.tenant_id"),
            "the read resolved its session variables, got: {pairs:?}"
        );
        assert!(
            pairs.iter().all(|(k, _)| k != fraiseql_db::STARTED_AT_VAR),
            "a read must not set the mutation timestamp, got: {pairs:?}"
        );
    }

    /// C-SV2: no session variables passed when `session_variables` config is empty.
    #[tokio::test]
    async fn test_no_session_variables_injected_when_config_empty() {
        let schema = test_schema(); // session_variables defaults to empty
        let adapter = Arc::new(SessionVarCapturingAdapter::new(mock_user_results()));
        let executor = Executor::read_only(schema, adapter.clone());

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

    /// A node-resolvable variant of [`schema_with_session_vars`]: the Relay `node(id:)`
    /// path resolves the type by name from `schema.types`, so the `User` type must be
    /// registered (the list-query-only `test_schema` does not register it).
    fn node_schema_with_session_vars() -> CompiledSchema {
        let mut schema = schema_with_session_vars();
        schema.types.push({
            let mut t = TypeDefinition::new("User", "v_user");
            t.fields = vec![
                FieldDefinition::new("id", FieldType::String),
                FieldDefinition::new("name", FieldType::String),
            ];
            t
        });
        schema
    }

    // the Relay node(id:) lookup must resolve session variables so a PostgreSQL
    // current_setting()-backed RLS policy constrains it — the same way regular queries
    // (test_session_variables_injected_on_read_query, above) and Relay pages already do.
    // Before the fix the node path called the non-session projection method, so no
    // session variables reached the connection (cross-tenant read on any leaked node id).
    #[tokio::test]
    async fn test_session_variables_injected_on_node_lookup() {
        let schema = node_schema_with_session_vars();
        let adapter = Arc::new(SessionVarCapturingAdapter::new(mock_user_results()));
        let executor = Executor::read_only(schema, adapter.clone());

        let node_id =
            crate::runtime::relay::encode_node_id("User", "11111111-1111-1111-1111-111111111111");
        let query = format!("{{ node(id: \"{node_id}\") {{ id name }} }}");

        let ctx = security_ctx_with_tenant(); // tenant-abc
        executor.execute_with_security(&query, None, &ctx).await.unwrap();

        let pairs = adapter.captured_pairs();
        let tenant = pairs.iter().find(|(k, _)| k == "app.tenant_id").map(|(_, v)| v.as_str());
        assert_eq!(
            tenant,
            Some("tenant-abc"),
            "node(id:) lookup must resolve the caller's tenant into session variables for \
             current_setting()-backed RLS; got: {pairs:?}"
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

// ── mod node_authz: Relay `node(id:)` authorization (H2 IDOR) ──────────────
//
// The `node(id:)` lookup resolves an arbitrary type by opaque global id, so it
// must apply the same `requires_role` / RLS / `inject_params` gates as the regular
// query path for the backing query. Before the fix it applied none of them — a
// leaked node id returned the row with no access control.
mod node_authz {
    use super::*;

    /// Schema exposing `User` (view `v_user`) via a single query, configurable for
    /// the three gates the node path enforces.
    fn node_user_schema(
        requires_role: Option<&str>,
        inject_params: IndexMap<String, InjectedParamSource>,
    ) -> CompiledSchema {
        let mut schema = CompiledSchema::new();
        schema.queries.push(QueryDefinition {
            function: None,

            requires_actor: Vec::new(),
            returns_count: false,
            name: "users".to_string(),
            return_type: "User".to_string(),
            returns_list: true,
            nullable: false,
            arguments: Vec::new(),
            sql_source: Some("v_user".to_string()),
            description: None,
            auto_params: AutoParams::default(),
            deprecation: None,
            jsonb_column: "data".to_string(),
            relay: false,
            relay_cursor_column: None,
            relay_cursor_type: CursorType::default(),
            inject_params,
            read_routing: crate::backend::types::ReadRouting::default(),
            cache_ttl_seconds: None,
            additional_views: vec![],
            requires_role: requires_role.map(str::to_string),
            rest_path: None,
            rest_method: None,
            rest_stream: false,
            native_columns: HashMap::new(),
            pagination_order: None,
        });
        schema.types.push({
            let mut t = TypeDefinition::new("User", "v_user");
            t.fields = vec![
                FieldDefinition::new("id", FieldType::String),
                FieldDefinition::new("name", FieldType::String),
            ];
            t
        });
        schema
    }

    /// A `{ node(id: <encoded "User:uuid">) { id name } }` query string.
    fn node_query() -> String {
        let id =
            crate::runtime::relay::encode_node_id("User", "11111111-1111-1111-1111-111111111111");
        format!("{{ node(id: \"{id}\") {{ id name }} }}")
    }

    fn ctx_with_roles(roles: &[&str]) -> SecurityContext {
        SecurityContext {
            user_id:          "user-1".into(),
            roles:            roles.iter().map(|r| (*r).to_string()).collect(),
            tenant_id:        Some("tenant-abc".into()),
            scopes:           vec![],
            attributes:       HashMap::from([(
                "tenant_id".to_string(),
                serde_json::json!("tenant-abc"),
            )]),
            request_id:       "req-1".to_string(),
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
    async fn node_requires_role_anonymous_is_not_found() {
        let schema = node_user_schema(Some("admin"), IndexMap::new());
        let adapter = Arc::new(CapturingMockAdapter::new(mock_user_results()));
        let executor = Executor::new(schema, adapter.clone());

        let err = executor.execute(&node_query(), None).await.unwrap_err();
        assert!(
            err.to_string().contains("not found"),
            "expected enumeration-hiding error, got: {err}"
        );
        assert!(
            adapter.captured_where().is_none(),
            "DB must not be queried when role check fails"
        );
    }

    #[tokio::test]
    async fn node_requires_role_authenticated_without_role_is_not_found() {
        let schema = node_user_schema(Some("admin"), IndexMap::new());
        let adapter = Arc::new(CapturingMockAdapter::new(mock_user_results()));
        let executor = Executor::new(schema, adapter.clone());

        let ctx = ctx_with_roles(&["viewer"]);
        let err = executor.execute_with_security(&node_query(), None, &ctx).await.unwrap_err();
        assert!(err.to_string().contains("not found"));
        assert!(adapter.captured_where().is_none());
    }

    #[tokio::test]
    async fn node_requires_role_with_role_resolves() {
        let schema = node_user_schema(Some("admin"), IndexMap::new());
        let adapter = Arc::new(CapturingMockAdapter::new(mock_user_results()));
        let executor = Executor::new(schema, adapter.clone());

        let ctx = ctx_with_roles(&["admin"]);
        let result = executor.execute_with_security(&node_query(), None, &ctx).await.unwrap();
        assert!(result["data"].get("node").is_some());
        assert!(adapter.captured_where().is_some(), "role holder reaches the DB");
    }

    #[tokio::test]
    async fn node_rls_anonymous_fails_closed() {
        let schema = node_user_schema(None, IndexMap::new());
        let adapter = Arc::new(CapturingMockAdapter::new(mock_user_results()));
        let config = RuntimeConfig::default().with_rls_policy(Arc::new(DefaultRLSPolicy::new()));
        let executor = Executor::with_config(schema, adapter.clone(), config);

        let result = executor.execute(&node_query(), None).await.unwrap();
        assert_eq!(
            result["data"]["node"],
            serde_json::Value::Null,
            "anonymous node lookup of an RLS-backed type must be null"
        );
        assert!(adapter.captured_where().is_none(), "DB must not be queried (fail closed)");
    }

    #[tokio::test]
    async fn node_rls_authenticated_applies_rls_filter() {
        let schema = node_user_schema(None, IndexMap::new());
        let adapter = Arc::new(CapturingMockAdapter::new(mock_user_results()));
        let config = RuntimeConfig::default().with_rls_policy(Arc::new(DefaultRLSPolicy::new()));
        let executor = Executor::with_config(schema, adapter.clone(), config);

        let ctx = ctx_with_roles(&["viewer"]);
        let _ = executor.execute_with_security(&node_query(), None, &ctx).await.unwrap();
        match adapter.captured_where().expect("authenticated node reaches the DB") {
            WhereClause::And(clauses) => {
                assert!(clauses.len() >= 2, "expected AND(rls, id), got {clauses:?}");
            },
            other => panic!("expected AND(rls, id), got {other:?}"),
        }
    }

    #[tokio::test]
    async fn node_inject_anonymous_fails_closed() {
        let mut inject = IndexMap::new();
        inject.insert("tenant_id".to_string(), InjectedParamSource::Jwt("tenant_id".to_string()));
        let schema = node_user_schema(None, inject);
        let adapter = Arc::new(CapturingMockAdapter::new(mock_user_results()));
        let executor = Executor::new(schema, adapter.clone());

        let result = executor.execute(&node_query(), None).await.unwrap();
        assert_eq!(result["data"]["node"], serde_json::Value::Null);
        assert!(adapter.captured_where().is_none());
    }

    #[tokio::test]
    async fn node_inject_authenticated_applies_filter() {
        let mut inject = IndexMap::new();
        inject.insert("tenant_id".to_string(), InjectedParamSource::Jwt("tenant_id".to_string()));
        let schema = node_user_schema(None, inject);
        let adapter = Arc::new(CapturingMockAdapter::new(mock_user_results()));
        let executor = Executor::new(schema, adapter.clone());

        let ctx = ctx_with_roles(&["viewer"]);
        let _ = executor.execute_with_security(&node_query(), None, &ctx).await.unwrap();
        match adapter.captured_where().expect("authenticated node reaches the DB") {
            WhereClause::And(clauses) => {
                assert!(clauses.len() >= 2, "expected AND(inject, id), got {clauses:?}");
            },
            other => panic!("expected AND(inject, id), got {other:?}"),
        }
    }
}

// ── mod explicit_arg_recasing: #486 end-to-end (GraphQL arg → WHERE) ──────────
//
// Mirrors the #456 mutation-input e2e (`CapturingFunctionCallAdapter`): drive a
// real GraphQL query through the executor and capture the WHERE clause the
// adapter receives, proving a camelCase explicit argument resolves to the
// snake_case JSONB column the stored data actually uses.
mod explicit_arg_recasing {
    use super::*;
    use crate::schema::ArgumentDefinition;

    /// Build a list query `orders(<arg>: String)` over `v_orders`.
    fn orders_schema_with_arg(arg_name: &str) -> CompiledSchema {
        let mut schema = CompiledSchema::new();
        schema.queries.push(QueryDefinition {
            function: None,

            requires_actor:      Vec::new(),
            returns_count:       false,
            name:                "orders".to_string(),
            return_type:         "Order".to_string(),
            returns_list:        true,
            nullable:            false,
            arguments:           vec![ArgumentDefinition::new(arg_name, FieldType::String)],
            sql_source:          Some("v_orders".to_string()),
            description:         None,
            auto_params:         AutoParams::default(),
            deprecation:         None,
            jsonb_column:        "data".to_string(),
            relay:               false,
            relay_cursor_column: None,
            relay_cursor_type:   CursorType::default(),
            inject_params:       IndexMap::default(),
            read_routing:        crate::backend::types::ReadRouting::default(),
            cache_ttl_seconds:   None,
            additional_views:    vec![],
            requires_role:       None,
            rest_path:           None,
            rest_method:         None,
            rest_stream:         false,
            native_columns:      HashMap::new(),
            pagination_order:    None,
        });
        schema
    }

    /// Extract the single `Field` clause, unwrapping a one-element `And`.
    fn single_field(clause: WhereClause) -> (Vec<String>, serde_json::Value) {
        match clause {
            WhereClause::Field { path, value, .. } => (path, value),
            WhereClause::And(mut inner) if inner.len() == 1 => single_field(inner.remove(0)),
            other => panic!("expected a single Field clause, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn multiword_camel_arg_filters_on_snake_column() {
        let schema = orders_schema_with_arg("organizationId");
        let adapter = Arc::new(CapturingMockAdapter::new(mock_user_results()));
        let executor = Executor::new(schema, adapter.clone());

        executor
            .execute("{ orders(organizationId: \"abc\") { id } }", None)
            .await
            .unwrap();

        let (path, value) =
            single_field(adapter.captured_where().expect("explicit arg reaches the DB"));
        assert_eq!(
            path,
            vec!["organization_id".to_string()],
            "must filter data->>'organization_id'"
        );
        assert_eq!(value, serde_json::json!("abc"));
    }

    #[tokio::test]
    async fn single_word_arg_is_unchanged() {
        let schema = orders_schema_with_arg("status");
        let adapter = Arc::new(CapturingMockAdapter::new(mock_user_results()));
        let executor = Executor::new(schema, adapter.clone());

        executor.execute("{ orders(status: \"open\") { id } }", None).await.unwrap();

        let (path, _) =
            single_field(adapter.captured_where().expect("explicit arg reaches the DB"));
        assert_eq!(path, vec!["status".to_string()]);
    }
}

// ── mod search_relevance: #1284, the ordering a `?search=` implies ────────
//
// `?search=` without `?sort=` answered 400 on every REST representation: the
// transport wrote `[{"_relevance":"desc"}]` into `arguments["orderBy"]`, a shape
// no consumer parses, and no relevance ordering existed to signal. The ordering
// now travels as a typed field on the `QueryMatch` — for the same reason
// `scope_where` does (#1170) — and is lowered here.
mod search_relevance {
    use super::*;
    use crate::backend::RelevanceOrder;

    fn users_match() -> crate::runtime::matcher::QueryMatch {
        crate::runtime::QueryMatcher::new(test_schema())
            .match_query("{ users { id name } }", None)
            .unwrap()
    }

    fn relevance() -> RelevanceOrder {
        RelevanceOrder {
            fields: vec!["name".to_string()],
            query:  "ada".to_string(),
        }
    }

    /// The control that makes the case below mean something: with no search, the
    /// direct read orders by nothing at all.
    #[tokio::test]
    async fn a_read_with_no_search_orders_by_nothing() {
        let adapter = Arc::new(CapturingMockAdapter::new(mock_user_results()));
        let executor = Executor::new(test_schema(), adapter.clone());

        executor.execute_query_direct(&users_match(), None, None, None).await.unwrap();

        assert_eq!(adapter.captured_order_by(), None);
    }

    /// A search with no client sort is ranked.
    #[tokio::test]
    async fn a_search_relevance_becomes_the_ordering() {
        let adapter = Arc::new(CapturingMockAdapter::new(mock_user_results()));
        let executor = Executor::new(test_schema(), adapter.clone());
        let qm = users_match().with_search_relevance(relevance());

        executor.execute_query_direct(&qm, None, None, None).await.unwrap();

        let captured = adapter.captured_order_by().expect("the read must be ordered");
        assert_eq!(captured.len(), 1, "one ordering, the rank: {captured:?}");
        assert_eq!(captured[0].relevance.as_ref(), Some(&relevance()));
        assert_eq!(
            captured[0].direction,
            crate::backend::OrderDirection::Desc,
            "most relevant first"
        );
    }

    /// A client's own sort wins, which is what the generated OpenAPI document
    /// promises: "ranked by relevance unless `sort` is specified".
    ///
    /// The transport already declines to attach a relevance order when the
    /// client named a sort, so this is the second of two locks — and the one
    /// that decides what happens if a future producer forgets the first.
    #[tokio::test]
    async fn an_explicit_ordering_wins_over_the_rank() {
        let adapter = Arc::new(CapturingMockAdapter::new(mock_user_results()));
        let executor = Executor::new(test_schema(), adapter.clone());
        let mut qm = users_match().with_search_relevance(relevance());
        qm.arguments.insert(
            "orderBy".to_string(),
            serde_json::json!([{ "field": "name", "direction": "ASC" }]),
        );

        executor.execute_query_direct(&qm, None, None, None).await.unwrap();

        let captured = adapter.captured_order_by().expect("the read must be ordered");
        assert_eq!(captured.len(), 1);
        assert_eq!(captured[0].field, "name");
        assert!(
            captured[0].relevance.is_none(),
            "the client's sort must not carry a ranking: {captured:?}"
        );
    }
}

// ── mod pagination_order: #1303, the order a page is cut in ──────────────
//
// An offset page is a slice of a sequence, and a read with no `ORDER BY` is not
// a sequence: two pages of the same relation can overlap and skip rows, under a
// `200`. The order is decided by the compiler (`QueryDefinition::pagination_order`)
// and lowered here, in its own channel — never through `arguments`, which is the
// client's surface (#1170, #1284).
mod pagination_order {
    use super::*;
    use crate::schema::PaginationOrder;

    /// `test_schema`'s `users`, whose compiled ordering is the JSONB identity.
    fn users_match(args: &[(&str, serde_json::Value)]) -> crate::runtime::matcher::QueryMatch {
        let mut qm = crate::runtime::QueryMatcher::new(test_schema())
            .match_query("{ users { id name } }", None)
            .unwrap();
        for (k, v) in args {
            qm.arguments.insert((*k).to_string(), v.clone());
        }
        qm
    }

    fn schema_with_order(order: Option<PaginationOrder>) -> CompiledSchema {
        let mut schema = test_schema();
        schema.queries[0].pagination_order = order;
        schema
    }

    async fn captured_for(
        schema: CompiledSchema,
        args: &[(&str, serde_json::Value)],
    ) -> Option<Vec<crate::backend::OrderByClause>> {
        let adapter = Arc::new(CapturingMockAdapter::new(mock_user_results()));
        let executor = Executor::new(schema.clone(), adapter.clone());
        let mut qm = crate::runtime::QueryMatcher::new(schema)
            .match_query("{ users { id name } }", None)
            .unwrap();
        for (k, v) in args {
            qm.arguments.insert((*k).to_string(), v.clone());
        }
        executor.execute_query_direct(&qm, None, None, None).await.unwrap();
        adapter.captured_order_by()
    }

    #[tokio::test]
    async fn a_paged_read_with_no_ordering_gets_the_declared_identity() {
        let captured = captured_for(test_schema(), &[("limit", serde_json::json!(2))])
            .await
            .expect("a paged read must be ordered");
        assert_eq!(captured.len(), 1, "{captured:?}");
        assert!(captured[0].identity, "the clause must be marked, or the renderer adds a second");
        assert_eq!(captured[0].field, "id");
        assert_eq!(captured[0].native_column, None, "the JSONB identity reads no column");
        assert_eq!(captured[0].direction, crate::backend::OrderDirection::Asc);
    }

    /// The control that makes the case above mean something: an unpaged read has
    /// no second page to overlap with, and sorting it would be a cost with no
    /// beneficiary.
    #[tokio::test]
    async fn an_unpaged_read_is_still_unordered() {
        assert_eq!(captured_for(test_schema(), &[]).await, None);
    }

    /// `?offset=` alone slices a suffix, which is as much a slice of a sequence as
    /// a prefix is.
    #[tokio::test]
    async fn an_offset_alone_is_still_a_page() {
        let captured = captured_for(test_schema(), &[("offset", serde_json::json!(10))])
            .await
            .expect("an offset read must be ordered");
        assert_eq!(captured.len(), 1);
        assert!(captured[0].identity);
    }

    /// A declared column reaches the adapter as a native column, so the renderer
    /// emits `pk_user ASC` rather than a JSONB extraction — the whole point of
    /// deciding this where the schema is visible.
    #[tokio::test]
    async fn a_declared_column_reaches_the_adapter_as_a_native_column() {
        let captured = captured_for(
            schema_with_order(Some(PaginationOrder::Column("pk_user".into()))),
            &[("limit", serde_json::json!(2))],
        )
        .await
        .expect("a paged read must be ordered");
        assert_eq!(captured.len(), 1);
        assert_eq!(captured[0].native_column.as_deref(), Some("pk_user"));
        assert!(captured[0].identity);
    }

    /// The declared opt-out: a view carrying its own `ORDER BY` keeps it.
    #[tokio::test]
    async fn a_query_that_declared_none_stays_unordered() {
        assert_eq!(
            captured_for(schema_with_order(None), &[("limit", serde_json::json!(2))]).await,
            None
        );
    }

    /// A client's sort is never replaced — the identity is appended, so it can
    /// only break ties the client's own keys left.
    #[tokio::test]
    async fn a_client_ordering_keeps_its_keys_and_gains_the_identity() {
        let captured = captured_for(
            test_schema(),
            &[
                ("limit", serde_json::json!(2)),
                ("orderBy", serde_json::json!([{ "field": "name", "direction": "DESC" }])),
            ],
        )
        .await
        .expect("ordered");
        assert_eq!(captured.len(), 2, "{captured:?}");
        assert_eq!(captured[0].field, "name");
        assert_eq!(captured[0].direction, crate::backend::OrderDirection::Desc);
        assert!(!captured[0].identity);
        assert!(captured[1].identity);
    }

    /// A client that sorted by the identity itself does not get a second copy —
    /// asked through the same predicate the renderer's tie-breaker uses.
    #[tokio::test]
    async fn a_client_ordering_by_id_is_not_given_a_second_copy() {
        let captured = captured_for(
            test_schema(),
            &[
                ("limit", serde_json::json!(2)),
                ("orderBy", serde_json::json!([{ "field": "id", "direction": "DESC" }])),
            ],
        )
        .await
        .expect("ordered");
        assert_eq!(captured.len(), 1, "{captured:?}");
        assert_eq!(captured[0].direction, crate::backend::OrderDirection::Desc, "the client's own");
    }

    /// The ordering is server-composed and never enters the argument map, which
    /// is the client's surface: a lowering that wrote it there would publish a
    /// value no client sent (#1170's rule, #1284's defect).
    #[tokio::test]
    async fn the_identity_never_enters_the_argument_map() {
        let qm = users_match(&[("limit", serde_json::json!(2))]);
        let adapter = Arc::new(CapturingMockAdapter::new(mock_user_results()));
        let executor = Executor::new(test_schema(), adapter.clone());
        executor.execute_query_direct(&qm, None, None, None).await.unwrap();

        assert!(adapter.captured_order_by().is_some(), "the read was ordered");
        assert!(
            !qm.arguments.contains_key("orderBy"),
            "the ordering must not be written back into the client's arguments: {:?}",
            qm.arguments
        );
    }

    /// Both GraphQL runners resolve the same read. An ordering applied on one
    /// entry point and not the other is the #739 shape — two readers of one query
    /// disagreeing about what it means.
    #[tokio::test]
    async fn the_anonymous_graphql_runner_orders_the_same_read() {
        let adapter = Arc::new(CapturingMockAdapter::new(mock_user_results()));
        let executor = Executor::new(test_schema(), adapter.clone());

        executor.execute("{ users(limit: 2) { id name } }", None).await.unwrap();

        let captured = adapter.captured_order_by().expect("a paged read must be ordered");
        assert_eq!(captured.len(), 1);
        assert!(captured[0].identity);
    }

    #[tokio::test]
    async fn the_authenticated_graphql_runner_orders_the_same_read() {
        let adapter = Arc::new(CapturingMockAdapter::new(mock_user_results()));
        let executor = Executor::new(test_schema(), adapter.clone());
        let ctx = SecurityContext {
            user_id:          "user-42".into(),
            roles:            vec!["viewer".to_string()],
            tenant_id:        None,
            scopes:           vec![],
            attributes:       HashMap::default(),
            request_id:       "req-1303".to_string(),
            ip_address:       None,
            expires_at:       Utc::now() + chrono::Duration::hours(1),
            authenticated_at: Utc::now(),
            issuer:           None,
            audience:         None,
            email:            None,
            display_name:     None,
        };

        executor
            .execute_with_security("{ users(limit: 2) { id name } }", None, &ctx)
            .await
            .unwrap();

        let captured = adapter.captured_order_by().expect("a paged read must be ordered");
        assert_eq!(captured.len(), 1);
        assert!(captured[0].identity);
    }

    /// #1304: the artifact this whole module can be defeated by. A 2.14 compile
    /// wrote no `pagination_order` on any query, and the runtime above reads a
    /// missing one as "this query declared no page order" — the deliberate
    /// opt-out. So the artifact boots, and every paged read silently goes back
    /// to the overlapping pages #1303 shipped to remove.
    ///
    /// It is refused before it can, because it was not produced by this build.
    #[tokio::test]
    async fn an_artifact_from_before_this_field_existed_is_refused_not_run() {
        let mut value = serde_json::to_value(test_schema()).unwrap();
        for q in value["queries"].as_array_mut().unwrap() {
            q.as_object_mut().unwrap().remove("pagination_order");
        }
        value.as_object_mut().unwrap().remove("fraiseql_version");
        let stale: CompiledSchema = serde_json::from_value(value).unwrap();

        // The shape is exactly the one that would be misread: nothing on any
        // query says how to order a page.
        assert!(stale.queries.iter().all(|q| q.pagination_order.is_none()));

        let err = RuntimeConfig::from_compiled_schema(&stale)
            .expect_err("an artifact this build did not produce must not reach the executor");
        assert!(err.contains("Recompile"), "{err}");
    }
}

// ── mod row_read: the row-shaped read faces the same gates (#1351) ────────
//
// gRPC answers row-shaped results — a `Vec<Vec<ColumnValue>>` projected through
// protobuf `ColumnSpec`s — and used to get them by calling
// `DatabaseAdapter::execute_row_query` itself, building its own WHERE clause. That
// made it a second read implementation, so the operation `Authorizer` (#422), the
// `requires_role` gate (#1122), the actor allow-list (#966), the field gate (#423)
// and the compiled page-size ceiling (#421) applied to every transport but that one.
//
// Every case here is a **pair**. A refusal-only test passes against a read arm that
// refuses everything, which is exactly the shape #1351 warns about.
mod row_read {
    use fraiseql_db::dialect::RowViewColumnType;

    use super::*;
    use crate::{
        backend::types::{ColumnSpec, ColumnValue},
        schema::{FieldDenyPolicy, SessionVariableMapping, SessionVariableSource},
        security::{Authorizer, AuthzDecision, AuthzRequest},
    };

    /// `User` with an ordinary field and, optionally, a policy-gated one.
    fn user_schema() -> CompiledSchema {
        let mut schema = test_schema();
        schema.types.push(TypeDefinition {
            fields: vec![
                FieldDefinition::new("id", FieldType::Id),
                FieldDefinition::new("name", FieldType::String),
            ],
            ..TypeDefinition::new("User", "tb_users")
        });
        schema.build_indexes();
        schema
    }

    /// The same schema, with `salary` gated on a field policy (#423).
    fn gated_schema() -> CompiledSchema {
        let mut schema = test_schema();
        let mut salary = FieldDefinition::new("salary", FieldType::Int);
        salary.authorize = true;
        schema.types.push(TypeDefinition {
            fields: vec![
                FieldDefinition::new("id", FieldType::Id),
                FieldDefinition::new("name", FieldType::String),
                salary,
            ],
            ..TypeDefinition::new("User", "tb_users")
        });
        schema.build_indexes();
        schema
    }

    fn cols(names: &[&str]) -> Vec<ColumnSpec> {
        names
            .iter()
            .map(|n| ColumnSpec {
                name:        (*n).to_string(),
                column_type: RowViewColumnType::Text,
            })
            .collect()
    }

    /// A principal carrying `read:User` and nothing else — notably not
    /// `read:salary`, which the masking case below depends on.
    fn principal() -> SecurityContext {
        SecurityContext {
            user_id:          "user-42".into(),
            roles:            vec!["viewer".to_string()],
            tenant_id:        Some("tenant-abc".into()),
            scopes:           vec!["read:User".to_string()],
            attributes:       HashMap::from([(
                "tenant_id".to_string(),
                serde_json::json!("tenant-abc"),
            )]),
            request_id:       "req-row".to_string(),
            ip_address:       None,
            expires_at:       Utc::now() + chrono::Duration::hours(1),
            authenticated_at: Utc::now(),
            issuer:           None,
            audience:         None,
            email:            None,
            display_name:     None,
        }
    }

    fn rows() -> Vec<Vec<ColumnValue>> {
        vec![vec![
            ColumnValue::Text("1".into()),
            ColumnValue::Text("Alice".into()),
        ]]
    }

    fn match_on(schema: &CompiledSchema, query: &str) -> crate::runtime::matcher::QueryMatch {
        crate::runtime::QueryMatcher::new(schema.clone())
            .match_query(query, None)
            .unwrap()
    }

    /// `match_query` seeds the argument map from the variables, which is the
    /// reachable spelling for `limit`/`offset`/`where` on this path.
    fn match_with(
        schema: &CompiledSchema,
        query: &str,
        vars: &serde_json::Value,
    ) -> crate::runtime::matcher::QueryMatch {
        crate::runtime::QueryMatcher::new(schema.clone())
            .match_query(query, Some(vars))
            .unwrap()
    }

    struct DenyAll;
    impl Authorizer for DenyAll {
        fn authorize(&self, _req: &AuthzRequest<'_>) -> crate::error::Result<AuthzDecision> {
            Ok(AuthzDecision::Deny {
                reason: "nope".into(),
            })
        }
    }

    struct AllowAll;
    impl Authorizer for AllowAll {
        fn authorize(&self, _req: &AuthzRequest<'_>) -> crate::error::Result<AuthzDecision> {
            Ok(AuthzDecision::Allow)
        }
    }

    fn with_authorizer(authz: Arc<dyn Authorizer>) -> RuntimeConfig {
        RuntimeConfig {
            authorizer: Some(authz),
            ..RuntimeConfig::default()
        }
    }

    // ---- the operation Authorizer (#422) ---------------------------------

    /// Denied: the read never reaches the database.
    #[tokio::test]
    async fn an_operation_the_authorizer_denies_never_reaches_the_database() {
        let schema = user_schema();
        let qm = match_on(&schema, "{ users { id name } }");
        let adapter = Arc::new(CapturingMockAdapter::new(vec![]).with_row_results(rows()));
        let executor =
            Executor::with_config(schema, adapter.clone(), with_authorizer(Arc::new(DenyAll)));

        let err = executor
            .execute_row_read(&qm, None, None, &cols(&["id", "name"]))
            .await
            .unwrap_err();

        assert!(
            matches!(err, crate::FraiseQLError::Authorization { .. }),
            "expected Authorization, got {err:?}"
        );
        assert!(
            adapter.captured_row_read().is_none(),
            "a denied read must not reach the adapter"
        );
    }

    /// Allowed: the very same read does reach it. Without this half, the case
    /// above would pass against an entry that refused everything.
    #[tokio::test]
    async fn the_same_read_a_permitted_principal_makes_reaches_the_database() {
        let schema = user_schema();
        let qm = match_on(&schema, "{ users { id name } }");
        let adapter = Arc::new(CapturingMockAdapter::new(vec![]).with_row_results(rows()));
        let executor =
            Executor::with_config(schema, adapter.clone(), with_authorizer(Arc::new(AllowAll)));

        let out = executor
            .execute_row_read(&qm, None, None, &cols(&["id", "name"]))
            .await
            .unwrap();

        assert_eq!(out.rows.len(), 1);
        let seen = adapter.captured_row_read().expect("the read must reach the adapter");
        assert_eq!(seen.view, "vr_tb_users");
    }

    /// The read targets the **row-shaped** view, not the query's `sql_source`.
    ///
    /// `sql_source` names the JSONB document view, whose only column is `data`.
    /// Reading it with the column extractor would not error — `SELECT *` succeeds
    /// and the extractor simply finds none of the names it wants — it would answer
    /// every field of every row as `NULL`. That is why this is pinned by name: the
    /// wrong view here is a silent wrong answer, not a failure.
    #[tokio::test]
    async fn the_read_targets_the_row_shaped_view_not_the_document_view() {
        let schema = user_schema();
        let qm = match_on(&schema, "{ users { id name } }");
        let adapter = Arc::new(CapturingMockAdapter::new(vec![]).with_row_results(rows()));
        let executor = Executor::new(schema, adapter.clone());

        executor
            .execute_row_read(&qm, None, None, &cols(&["id", "name"]))
            .await
            .unwrap();

        let seen = adapter.captured_row_read().expect("the read must reach the adapter");
        assert_eq!(
            seen.view, "vr_tb_users",
            "the row read takes the type's row-shaped view, not the query's `v_user`"
        );
    }

    // ---- the compiled page-size ceiling (#421) ---------------------------

    /// Over the ceiling: refused, and nothing is read.
    ///
    /// `enforce_max_page_size` **refuses** rather than silently capping, so the
    /// assertion is a `Validation` error plus an untouched adapter — not a
    /// clamped limit.
    #[tokio::test]
    async fn a_page_larger_than_the_compiled_ceiling_is_refused() {
        let schema = user_schema();
        let qm = match_with(&schema, "{ users { id name } }", &serde_json::json!({"limit": 5000}));
        let adapter = Arc::new(CapturingMockAdapter::new(vec![]).with_row_results(rows()));
        let executor = Executor::new(schema, adapter.clone());

        let err = executor
            .execute_row_read(&qm, None, None, &cols(&["id", "name"]))
            .await
            .unwrap_err();

        match err {
            crate::FraiseQLError::Validation { message, .. } => {
                assert!(message.contains("maximum page size"), "message was: {message}");
            },
            other => panic!("expected Validation, got {other:?}"),
        }
        assert!(adapter.captured_row_read().is_none(), "refused before dispatch");
    }

    /// At the ceiling: read, and the limit travels unchanged.
    #[tokio::test]
    async fn a_page_at_the_compiled_ceiling_is_read() {
        let schema = user_schema();
        let qm = match_with(&schema, "{ users { id name } }", &serde_json::json!({"limit": 1000}));
        let adapter = Arc::new(CapturingMockAdapter::new(vec![]).with_row_results(rows()));
        let executor = Executor::new(schema, adapter.clone());

        executor
            .execute_row_read(&qm, None, None, &cols(&["id", "name"]))
            .await
            .unwrap();

        let seen = adapter.captured_row_read().expect("the read must reach the adapter");
        assert_eq!(seen.limit, Some(1000));
    }

    /// Everything the chokepoint resolved is lowered into the row call — the
    /// client predicate, the declared page ordering (#1303) and the offset.
    ///
    /// Without this the row entry could resolve a predicate correctly and hand the
    /// adapter `None`, which is the exact shape of a gate that runs and is then
    /// ignored: the read would return every row while every gate reported success.
    #[tokio::test]
    async fn the_resolved_predicate_ordering_and_offset_are_lowered_to_the_row_read() {
        let schema = user_schema();
        let qm = match_with(
            &schema,
            "{ users { id name } }",
            &serde_json::json!({
                "limit": 10,
                "offset": 5,
                "where": {"name": {"eq": "Alice"}}
            }),
        );
        let adapter = Arc::new(CapturingMockAdapter::new(vec![]).with_row_results(rows()));
        let executor = Executor::new(schema, adapter.clone());

        executor
            .execute_row_read(&qm, None, None, &cols(&["id", "name"]))
            .await
            .unwrap();

        let seen = adapter.captured_row_read().expect("the read must reach the adapter");
        assert_eq!(seen.offset, Some(5), "the offset travels");
        assert_eq!(seen.limit, Some(10), "the limit travels");
        let where_sql = seen.where_sql.expect("the client predicate must reach the read");
        assert!(where_sql.contains("Alice"), "predicate was: {where_sql}");
        assert!(
            seen.order_by.is_some(),
            "a paginated read carries the declared ordering (#1303)"
        );
    }

    // ---- the field gate (#423) -------------------------------------------

    /// A policy-gated field in the projection is refused on this path, exactly as
    /// it is on the REST direct read: the row path does not run the per-row
    /// authorizer, so it fails closed rather than serving the value.
    #[tokio::test]
    async fn a_gated_field_in_the_projection_is_refused() {
        let schema = gated_schema();
        let qm = match_on(&schema, "{ users { id name salary } }");
        let adapter = Arc::new(CapturingMockAdapter::new(vec![]).with_row_results(rows()));
        let executor = Executor::new(schema, adapter.clone());

        let err = executor
            .execute_row_read(&qm, None, None, &cols(&["id", "name", "salary"]))
            .await
            .unwrap_err();

        assert!(
            matches!(err, crate::FraiseQLError::Authorization { .. }),
            "expected Authorization, got {err:?}"
        );
        assert!(adapter.captured_row_read().is_none(), "the gated value must never be read");
    }

    /// The same schema, the same transport, a projection without the gated field:
    /// served. The refusal above is about the field, not about the path.
    #[tokio::test]
    async fn the_same_read_without_the_gated_field_is_served() {
        let schema = gated_schema();
        let qm = match_on(&schema, "{ users { id name } }");
        let adapter = Arc::new(CapturingMockAdapter::new(vec![]).with_row_results(rows()));
        let executor = Executor::new(schema, adapter.clone());

        let out = executor
            .execute_row_read(&qm, None, None, &cols(&["id", "name"]))
            .await
            .unwrap();

        assert_eq!(out.rows.len(), 1);
        assert_eq!(out.columns.iter().map(|c| c.name.as_str()).collect::<Vec<_>>(), ["id", "name"]);
    }

    // ---- field-level RBAC narrows the projection (#886) ------------------

    /// A masked field is not read at all, and the columns that come back describe
    /// the row that came back.
    ///
    /// The alignment half is the load-bearing one: the adapter zips values to
    /// specs positionally, so a caller that encoded with the specs it *asked* for
    /// while the read used a narrower set would serve one field's value under
    /// another field's name.
    #[tokio::test]
    async fn a_masked_field_is_not_read_and_the_columns_match_the_rows() {
        let mut schema = test_schema();
        let mut salary = FieldDefinition::new("salary", FieldType::Int);
        salary.requires_scope = Some("read:salary".to_string());
        salary.on_deny = FieldDenyPolicy::Mask;
        schema.types.push(TypeDefinition {
            fields: vec![
                FieldDefinition::new("id", FieldType::Id),
                FieldDefinition::new("name", FieldType::String),
                salary,
            ],
            ..TypeDefinition::new("User", "tb_users")
        });
        // Field-level RBAC is inert unless the schema declares a security section —
        // `apply_field_rbac_filtering` returns "everything projected, nothing masked"
        // when it is absent, so a fixture without one would assert the permissive
        // shape and pass no matter what the row path did.
        schema.security = Some(crate::schema::SecurityConfig::default());
        schema.build_indexes();

        let qm = match_on(&schema, "{ users { id name salary } }");
        let adapter = Arc::new(CapturingMockAdapter::new(vec![]).with_row_results(rows()));
        let executor = Executor::new(schema, adapter.clone());

        // A principal without `read:salary`.
        let ctx = principal();
        let out = executor
            .execute_row_read(&qm, None, Some(&ctx), &cols(&["id", "name", "salary"]))
            .await
            .unwrap();

        let seen = adapter.captured_row_read().expect("the read must reach the adapter");
        assert!(
            !seen.columns.iter().any(|c| c == "salary"),
            "a masked field must not be read: {:?}",
            seen.columns
        );
        assert_eq!(
            out.columns.iter().map(|c| c.name.clone()).collect::<Vec<_>>(),
            seen.columns,
            "the columns handed back must be the ones the read used"
        );
    }

    /// Ruling Y 7: a `requires_scope` is a gate whether or not the schema carries a
    /// `security` section. A scope is granted only by a role definition, and a schema
    /// without the section defines none — so no principal holds `read:salary`, and the
    /// field is masked, not served.
    #[tokio::test]
    async fn a_scoped_field_is_masked_without_a_security_section() {
        let mut schema = test_schema();
        let mut salary = FieldDefinition::new("salary", FieldType::Int);
        salary.requires_scope = Some("read:salary".to_string());
        salary.on_deny = FieldDenyPolicy::Mask;
        schema.types.push(TypeDefinition {
            fields: vec![
                FieldDefinition::new("id", FieldType::Id),
                FieldDefinition::new("name", FieldType::String),
                salary,
            ],
            ..TypeDefinition::new("User", "tb_users")
        });
        assert!(schema.security.is_none(), "precondition: no security section");
        schema.build_indexes();

        let qm = match_on(&schema, "{ users { id name salary } }");
        let adapter = Arc::new(CapturingMockAdapter::new(vec![]).with_row_results(rows()));
        let executor = Executor::new(schema, adapter.clone());

        let ctx = principal();
        executor
            .execute_row_read(&qm, None, Some(&ctx), &cols(&["id", "name", "salary"]))
            .await
            .unwrap();

        let seen = adapter.captured_row_read().expect("the read must reach the adapter");
        assert!(
            !seen.columns.iter().any(|c| c == "salary"),
            "no role grants `read:salary`, so it must not be read: {:?}",
            seen.columns
        );
    }

    /// The same, with no principal: an anonymous read is denied every scope whether or not
    /// the schema has a `security` section (#743), so the section's absence must not open it.
    #[tokio::test]
    async fn a_scoped_field_is_masked_for_an_anonymous_read_without_a_security_section() {
        let mut schema = test_schema();
        let mut salary = FieldDefinition::new("salary", FieldType::Int);
        salary.requires_scope = Some("read:salary".to_string());
        salary.on_deny = FieldDenyPolicy::Mask;
        schema.types.push(TypeDefinition {
            fields: vec![
                FieldDefinition::new("id", FieldType::Id),
                FieldDefinition::new("name", FieldType::String),
                salary,
            ],
            ..TypeDefinition::new("User", "tb_users")
        });
        assert!(schema.security.is_none(), "precondition: no security section");
        schema.build_indexes();

        let qm = match_on(&schema, "{ users { id name salary } }");
        let adapter = Arc::new(CapturingMockAdapter::new(vec![]).with_row_results(rows()));
        let executor = Executor::new(schema, adapter.clone());

        executor
            .execute_row_read(&qm, None, None, &cols(&["id", "name", "salary"]))
            .await
            .unwrap();

        let seen = adapter.captured_row_read().expect("the read must reach the adapter");
        assert!(
            !seen.columns.iter().any(|c| c == "salary"),
            "an anonymous caller holds no scope, so it must not be read: {:?}",
            seen.columns
        );
    }

    /// Ruling AA 3: the row read (the gRPC path) drops a masked column from what it reads —
    /// and must not filter by it either: which rows come back answers a question about the
    /// value the caller may not read.
    #[tokio::test]
    async fn the_row_read_refuses_a_filter_on_a_masked_field() {
        let mut schema = test_schema();
        let mut salary = FieldDefinition::new("salary", FieldType::Int);
        salary.requires_scope = Some("read:salary".to_string());
        salary.on_deny = FieldDenyPolicy::Mask;
        schema.types.push(TypeDefinition {
            fields: vec![
                FieldDefinition::new("id", FieldType::Id),
                FieldDefinition::new("name", FieldType::String),
                salary,
            ],
            ..TypeDefinition::new("User", "tb_users")
        });
        schema.security = Some(crate::schema::SecurityConfig::default());
        schema.build_indexes();

        let qm = match_on(&schema, "{ users(where: { salary: { gt: 100000 } }) { id name } }");
        let adapter = Arc::new(CapturingMockAdapter::new(vec![]).with_row_results(rows()));
        let executor = Executor::new(schema, adapter.clone());

        let res = executor
            .execute_row_read(&qm, None, Some(&principal()), &cols(&["id", "name"]))
            .await;
        assert!(
            matches!(res, Err(crate::error::FraiseQLError::Authorization { .. })),
            "a filter on a masked column answers a question about its value: {:?}",
            res.map(|_| ())
        );
        assert!(adapter.captured_row_read().is_none(), "the read must not reach the database");
    }

    // ---- session variables reach the read (#329) -------------------------

    /// A resolved session variable travels to the adapter.
    ///
    /// The row path had no session-pinned method at all before #1351, so the
    /// variables the chokepoint resolves had nowhere to go. A resolved value the
    /// read cannot apply is the failure the chokepoint exists to prevent, so this
    /// asserts the pair `(name, value)` arrives — not merely that some call happened.
    #[tokio::test]
    async fn the_resolved_session_variables_reach_the_row_read() {
        let mut schema = user_schema();
        schema.session_variables.variables.push(SessionVariableMapping {
            name:   "app.tenant_id".to_string(),
            source: SessionVariableSource::Literal {
                value: "tenant-abc".to_string(),
            },
        });
        schema.build_indexes();

        let qm = match_on(&schema, "{ users { id name } }");
        let adapter = Arc::new(CapturingMockAdapter::new(vec![]).with_row_results(rows()));
        let executor = Executor::new(schema, adapter.clone());

        let ctx = principal();
        executor
            .execute_row_read(&qm, None, Some(&ctx), &cols(&["id", "name"]))
            .await
            .unwrap();

        let seen = adapter.captured_row_read().expect("the read must reach the adapter");
        assert_eq!(
            seen.session_vars,
            vec![("app.tenant_id".to_string(), "tenant-abc".to_string())],
            "the session variable must reach the read's connection"
        );
    }

    // ---- the dynamic field authorizer, per row (#423) --------------------
    //
    // Before #1351's second half the row path called `deny_if_gated_field_selected`,
    // which refused a gated field for *every* principal. The pair below is the one the
    // issue asks for and the one a blanket refusal cannot pass: rejected → the field
    // comes back null, accepted → the field comes back with its value. A test that only
    // asserted the refusal would have stayed green against the placeholder.

    /// Reveals `salary` only to `owner`; masks it for anyone else.
    struct SalaryForOwnerOnly;
    impl crate::security::FieldAuthorizer for SalaryForOwnerOnly {
        fn authorize_field(
            &self,
            req: &crate::security::FieldAuthzRequest<'_>,
        ) -> crate::error::Result<crate::security::FieldAuthzDecision> {
            if req.principal.user_id.as_str() == "owner" {
                Ok(crate::security::FieldAuthzDecision::Allow)
            } else {
                Ok(crate::security::FieldAuthzDecision::Deny {
                    code:    "not_owner".to_string(),
                    on_deny: FieldDenyPolicy::Mask,
                })
            }
        }
    }

    /// Rejects `salary` outright, whoever asks.
    struct SalaryRejected;
    impl crate::security::FieldAuthorizer for SalaryRejected {
        fn authorize_field(
            &self,
            _req: &crate::security::FieldAuthzRequest<'_>,
        ) -> crate::error::Result<crate::security::FieldAuthzDecision> {
            Ok(crate::security::FieldAuthzDecision::Deny {
                code:    "never".to_string(),
                on_deny: FieldDenyPolicy::Reject,
            })
        }
    }

    fn with_field_authorizer(authz: Arc<dyn crate::security::FieldAuthorizer>) -> RuntimeConfig {
        RuntimeConfig {
            field_authorizer: Some(authz),
            ..RuntimeConfig::default()
        }
    }

    fn principal_named(user_id: &str) -> SecurityContext {
        SecurityContext {
            user_id: user_id.into(),
            ..principal()
        }
    }

    /// `ColumnValue` is a wire type in `fraiseql-db` and carries no `PartialEq`, so
    /// these cases compare its `Debug` rendering. That distinguishes a value from
    /// `Null` and from a different value, which is all they turn on — and it is
    /// cheaper than widening another crate's public API for a test.
    fn shown(v: &ColumnValue) -> String {
        format!("{v:?}")
    }

    /// Three columns, so the masked slot's position is observable.
    fn salary_rows() -> Vec<Vec<ColumnValue>> {
        vec![vec![
            ColumnValue::Text("1".into()),
            ColumnValue::Text("Alice".into()),
            ColumnValue::Int32(90_000),
        ]]
    }

    /// Rejected principal: the read runs, and the gated slot comes back `Null`.
    ///
    /// The row is still served — `Deny { on_deny: Mask }` is a statement about the
    /// value, not about the operation — so the assertions are positional: `id` and
    /// `name` keep their values and only index 2 is nulled. A masking bug that nulled
    /// the wrong slot would pass a "salary is null" assertion on its own.
    #[tokio::test]
    async fn a_gated_field_the_authorizer_rejects_comes_back_null() {
        let schema = gated_schema();
        let qm = match_on(&schema, "{ users { id name salary } }");
        let adapter = Arc::new(CapturingMockAdapter::new(vec![]).with_row_results(salary_rows()));
        let executor = Executor::with_config(
            schema,
            adapter.clone(),
            with_field_authorizer(Arc::new(SalaryForOwnerOnly)),
        );

        let ctx = principal_named("stranger");
        let out = executor
            .execute_row_read(&qm, None, Some(&ctx), &cols(&["id", "name", "salary"]))
            .await
            .expect("a Mask decision serves the row");

        assert_eq!(out.columns.len(), 3, "the spec list is not narrowed per row");
        assert_eq!(shown(&out.rows[0][0]), r#"Text("1")"#);
        assert_eq!(shown(&out.rows[0][1]), r#"Text("Alice")"#);
        assert_eq!(shown(&out.rows[0][2]), "Null", "the gated slot is masked");
    }

    /// Accepted principal: the same read, the same schema, the value present.
    ///
    /// This is the half #1351 calls for and the placeholder could never satisfy.
    #[tokio::test]
    async fn the_same_gated_field_is_present_for_a_principal_the_authorizer_accepts() {
        let schema = gated_schema();
        let qm = match_on(&schema, "{ users { id name salary } }");
        let adapter = Arc::new(CapturingMockAdapter::new(vec![]).with_row_results(salary_rows()));
        let executor = Executor::with_config(
            schema,
            adapter.clone(),
            with_field_authorizer(Arc::new(SalaryForOwnerOnly)),
        );

        let ctx = principal_named("owner");
        let out = executor
            .execute_row_read(&qm, None, Some(&ctx), &cols(&["id", "name", "salary"]))
            .await
            .expect("an Allow decision serves the value");

        assert_eq!(
            shown(&out.rows[0][2]),
            "Int32(90000)",
            "an accepted principal must see the gated value"
        );
    }

    /// A `Reject` policy refuses the whole read, and no row is served.
    #[tokio::test]
    async fn a_reject_decision_refuses_the_whole_row_read() {
        let schema = gated_schema();
        let qm = match_on(&schema, "{ users { id name salary } }");
        let adapter = Arc::new(CapturingMockAdapter::new(vec![]).with_row_results(salary_rows()));
        let executor = Executor::with_config(
            schema,
            adapter.clone(),
            with_field_authorizer(Arc::new(SalaryRejected)),
        );

        let ctx = principal_named("owner");
        let err = executor
            .execute_row_read(&qm, None, Some(&ctx), &cols(&["id", "name", "salary"]))
            .await
            .unwrap_err();

        assert!(
            matches!(err, crate::FraiseQLError::Authorization { .. }),
            "expected Authorization, got {err:?}"
        );
    }

    /// No principal, a gated field selected, an authorizer configured: refused.
    ///
    /// A per-row policy decision needs someone to decide about. The anonymous caller is
    /// exactly who must not be served, so this stays fail-closed even though the path
    /// can now adjudicate.
    #[tokio::test]
    async fn a_gated_field_with_no_principal_is_refused_even_with_an_authorizer() {
        let schema = gated_schema();
        let qm = match_on(&schema, "{ users { id name salary } }");
        let adapter = Arc::new(CapturingMockAdapter::new(vec![]).with_row_results(salary_rows()));
        let executor = Executor::with_config(
            schema,
            adapter.clone(),
            with_field_authorizer(Arc::new(SalaryForOwnerOnly)),
        );

        let err = executor
            .execute_row_read(&qm, None, None, &cols(&["id", "name", "salary"]))
            .await
            .unwrap_err();

        assert!(
            matches!(err, crate::FraiseQLError::Authorization { .. }),
            "expected Authorization, got {err:?}"
        );
        assert!(adapter.captured_row_read().is_none(), "refused before dispatch");
    }

    // ---- the same pair on the streaming arm ------------------------------
    //
    // #1348 found these two arms disagreeing about a failing RLS evaluation, so every
    // gate is asserted on both. The streaming arm is also where a missing decision costs
    // the most: it is the arm with no bound on how many frames it emits.

    /// Drain a streamed row read into rows.
    async fn drain(
        read: crate::runtime::StreamedRowRead,
    ) -> crate::error::Result<Vec<Vec<ColumnValue>>> {
        use futures::StreamExt as _;
        let mut stream = read.stream;
        let mut out = Vec::new();
        while let Some(row) = stream.next().await {
            out.push(row?);
        }
        Ok(out)
    }

    /// Rejected principal, streaming: the frame carries a masked slot.
    #[tokio::test]
    async fn a_streamed_gated_field_the_authorizer_rejects_comes_back_null() {
        let schema = gated_schema();
        let qm = match_on(&schema, "{ users { id name salary } }");
        let adapter = Arc::new(CapturingMockAdapter::new(vec![]).with_row_results(salary_rows()));
        let executor = Executor::with_config(
            schema,
            adapter.clone(),
            with_field_authorizer(Arc::new(SalaryForOwnerOnly)),
        );

        let ctx = principal_named("stranger");
        let read = executor
            .stream_row_read(&qm, None, Some(&ctx), &cols(&["id", "name", "salary"]))
            .await
            .expect("a Mask decision serves the stream");
        let rows = drain(read).await.expect("no frame errors");

        assert_eq!(rows.len(), 1);
        assert_eq!(shown(&rows[0][1]), r#"Text("Alice")"#);
        assert_eq!(shown(&rows[0][2]), "Null", "the gated slot is masked per frame");
    }

    /// Accepted principal, streaming: the value is in the frame.
    #[tokio::test]
    async fn the_same_streamed_gated_field_is_present_for_an_accepted_principal() {
        let schema = gated_schema();
        let qm = match_on(&schema, "{ users { id name salary } }");
        let adapter = Arc::new(CapturingMockAdapter::new(vec![]).with_row_results(salary_rows()));
        let executor = Executor::with_config(
            schema,
            adapter.clone(),
            with_field_authorizer(Arc::new(SalaryForOwnerOnly)),
        );

        let ctx = principal_named("owner");
        let read = executor
            .stream_row_read(&qm, None, Some(&ctx), &cols(&["id", "name", "salary"]))
            .await
            .expect("an Allow decision serves the stream");
        let rows = drain(read).await.expect("no frame errors");

        assert_eq!(
            shown(&rows[0][2]),
            "Int32(90000)",
            "an accepted principal must see the gated value on the streaming arm too"
        );
    }

    /// A `Reject` policy ends the stream with an error rather than emitting the frame.
    #[tokio::test]
    async fn a_reject_decision_fails_the_stream() {
        let schema = gated_schema();
        let qm = match_on(&schema, "{ users { id name salary } }");
        let adapter = Arc::new(CapturingMockAdapter::new(vec![]).with_row_results(salary_rows()));
        let executor = Executor::with_config(
            schema,
            adapter.clone(),
            with_field_authorizer(Arc::new(SalaryRejected)),
        );

        let ctx = principal_named("owner");
        let read = executor
            .stream_row_read(&qm, None, Some(&ctx), &cols(&["id", "name", "salary"]))
            .await
            .expect("the refusal is per frame, so opening the stream succeeds");
        let err = drain(read).await.unwrap_err();

        assert!(
            matches!(err, crate::FraiseQLError::Authorization { .. }),
            "expected Authorization, got {err:?}"
        );
    }

    // ---- the compiled cost ceiling (#379) --------------------------------
    //
    // `per_request_max` was enforced only in `run_gate1`, which scores a parsed GraphQL
    // document. A direct read has none, so neither gRPC arm nor any REST read was
    // scored. #1351's table marked this ✅ for the engine read path; it was not, which
    // is why routing the arms through the engine did not deliver it.

    fn with_cost_cap(cap: u64) -> RuntimeConfig {
        RuntimeConfig {
            max_operation_cost: Some(cap),
            ..RuntimeConfig::default()
        }
    }

    /// Over the ceiling: refused, and the database is never reached.
    ///
    /// Two selected fields at `limit: 50` scores `1 + 2 × 50 = 101`, so a cap of 100
    /// refuses it — the same arithmetic `estimate_query_cost` applies to the equivalent
    /// document, which is the point of scoring it this way rather than inventing a scale.
    #[tokio::test]
    async fn a_row_read_over_the_compiled_cost_ceiling_is_refused() {
        let schema = user_schema();
        let qm = match_with(&schema, "{ users { id name } }", &serde_json::json!({"limit": 50}));
        let adapter = Arc::new(CapturingMockAdapter::new(vec![]).with_row_results(rows()));
        let executor = Executor::with_config(schema, adapter.clone(), with_cost_cap(100));

        let err = executor
            .execute_row_read(&qm, None, None, &cols(&["id", "name"]))
            .await
            .unwrap_err();

        match err {
            crate::FraiseQLError::CostExceeded { cost, limit, .. } => {
                assert_eq!(cost, 101, "1 + 2 fields x 50 rows");
                assert_eq!(limit, 100);
            },
            other => panic!("expected CostExceeded, got {other:?}"),
        }
        assert!(adapter.captured_row_read().is_none(), "an over-budget read must not run");
    }

    /// Under the ceiling: read. Without this half the case above would pass against a
    /// path that refused every read.
    #[tokio::test]
    async fn a_row_read_under_the_compiled_cost_ceiling_is_read() {
        let schema = user_schema();
        let qm = match_with(&schema, "{ users { id name } }", &serde_json::json!({"limit": 49}));
        let adapter = Arc::new(CapturingMockAdapter::new(vec![]).with_row_results(rows()));
        let executor = Executor::with_config(schema, adapter.clone(), with_cost_cap(100));

        executor
            .execute_row_read(&qm, None, None, &cols(&["id", "name"]))
            .await
            .expect("1 + 2 x 49 = 99, under the cap");

        assert!(adapter.captured_row_read().is_some(), "an in-budget read must run");
    }

    /// The streaming arm is scored too — the arm where an unbounded read costs most.
    #[tokio::test]
    async fn a_streamed_row_read_over_the_compiled_cost_ceiling_is_refused() {
        let schema = user_schema();
        let qm = match_with(&schema, "{ users { id name } }", &serde_json::json!({"limit": 50}));
        let adapter = Arc::new(CapturingMockAdapter::new(vec![]).with_row_results(rows()));
        let executor = Executor::with_config(schema, adapter.clone(), with_cost_cap(100));

        // `StreamedRowRead` holds a stream and carries no `Debug`, so the Result
        // cannot be `unwrap_err`'d.
        let Err(err) = executor.stream_row_read(&qm, None, None, &cols(&["id", "name"])).await
        else {
            panic!("an over-budget stream must not open")
        };

        assert!(
            matches!(err, crate::FraiseQLError::CostExceeded { .. }),
            "expected CostExceeded, got {err:?}"
        );
        assert!(adapter.captured_row_read().is_none(), "the stream must never open");
    }

    /// And the permitted twin on the streaming arm.
    #[tokio::test]
    async fn a_streamed_row_read_under_the_compiled_cost_ceiling_is_read() {
        let schema = user_schema();
        let qm = match_with(&schema, "{ users { id name } }", &serde_json::json!({"limit": 49}));
        let adapter = Arc::new(CapturingMockAdapter::new(vec![]).with_row_results(rows()));
        let executor = Executor::with_config(schema, adapter.clone(), with_cost_cap(100));

        let read = executor
            .stream_row_read(&qm, None, None, &cols(&["id", "name"]))
            .await
            .expect("under the cap");
        let rows = drain(read).await.expect("no frame errors");

        assert_eq!(rows.len(), 1);
    }

    /// An `@cost` weight on the query name is the whole score, subtree unwalked.
    ///
    /// The declared-weight branch, which the arithmetic cases above never reach. It is
    /// the branch that matters most for parity: `root_cost` gives a weighted root field
    /// exactly its weight and does not walk it, so a direct read must too — otherwise
    /// the same query scores one number as a document and another as a REST or gRPC read.
    #[tokio::test]
    async fn a_declared_cost_weight_is_the_whole_score() {
        let mut schema = user_schema();
        schema.operation_cost_weights.insert("users".to_string(), 500);
        let qm = match_with(&schema, "{ users { id name } }", &serde_json::json!({"limit": 2}));
        let adapter = Arc::new(CapturingMockAdapter::new(vec![]).with_row_results(rows()));
        let executor = Executor::with_config(schema, adapter.clone(), with_cost_cap(100));

        let err = executor
            .execute_row_read(&qm, None, None, &cols(&["id", "name"]))
            .await
            .unwrap_err();

        match err {
            crate::FraiseQLError::CostExceeded { cost, .. } => {
                // 1 + 2 fields x 2 rows = 5 would be well under the cap. The weight wins.
                assert_eq!(cost, 500, "the declared weight is the score, not the field count");
            },
            other => panic!("expected CostExceeded, got {other:?}"),
        }
    }

    /// The page multiplier clamps at 100, as it does for a document.
    ///
    /// `limit: 500` scores `1 + 2 x 100`, not `1 + 2 x 500` — the same ceiling
    /// `extract_limit_multiplier` applies. Pinned because an unclamped direct read would
    /// refuse pages a document of the same shape is served, which is the asymmetry this
    /// change exists to remove.
    #[tokio::test]
    async fn the_page_multiplier_clamps_at_a_hundred() {
        let schema = user_schema();
        let qm = match_with(&schema, "{ users { id name } }", &serde_json::json!({"limit": 500}));
        let adapter = Arc::new(CapturingMockAdapter::new(vec![]).with_row_results(rows()));
        let executor = Executor::with_config(schema, adapter.clone(), with_cost_cap(200));

        let err = executor
            .execute_row_read(&qm, None, None, &cols(&["id", "name"]))
            .await
            .unwrap_err();

        match err {
            crate::FraiseQLError::CostExceeded { cost, .. } => {
                assert_eq!(cost, 201, "1 + 2 fields x 100 (clamped), not x 500");
            },
            other => panic!("expected CostExceeded, got {other:?}"),
        }
    }

    /// A read with no cap configured is not scored at all — the gate is the operator's
    /// declaration, not a default ceiling this change introduces.
    #[tokio::test]
    async fn a_deployment_with_no_declared_ceiling_is_unscored() {
        let schema = user_schema();
        let qm = match_with(&schema, "{ users { id name } }", &serde_json::json!({"limit": 1000}));
        let adapter = Arc::new(CapturingMockAdapter::new(vec![]).with_row_results(rows()));
        let executor = Executor::new(schema, adapter.clone());

        executor
            .execute_row_read(&qm, None, None, &cols(&["id", "name"]))
            .await
            .expect("no cap declared, so nothing to exceed");

        assert!(adapter.captured_row_read().is_some());
    }

    // ---- the response-bytes ceiling ([validation] max_response_bytes) ----
    //
    // Depth and complexity score a document and bind only where one exists. Neither
    // knows what a row weighs, and a read from a materialised view is one fetch
    // whatever its nesting — so what bounds it is the bytes it returns. This is the
    // one control in the table on `resolve_gate1` that means the same thing on every
    // transport, so every transport is pinned here.

    fn with_response_bytes(cap: u64) -> RuntimeConfig {
        RuntimeConfig {
            max_response_bytes: Some(cap),
            ..RuntimeConfig::default()
        }
    }

    /// Three frames of `rows()`, which is `Text("1") + Text("Alice")` = 6 bytes each.
    fn three_rows() -> Vec<Vec<ColumnValue>> {
        vec![
            vec![
                ColumnValue::Text("1".into()),
                ColumnValue::Text("Alice".into()),
            ],
            vec![
                ColumnValue::Text("2".into()),
                ColumnValue::Text("Bobby".into()),
            ],
            vec![
                ColumnValue::Text("3".into()),
                ColumnValue::Text("Carol".into()),
            ],
        ]
    }

    /// Over the ceiling: refused once the delivered bytes cross it.
    #[tokio::test]
    async fn a_row_read_over_the_response_byte_ceiling_is_refused() {
        let schema = user_schema();
        let qm = match_on(&schema, "{ users { id name } }");
        let adapter = Arc::new(CapturingMockAdapter::new(vec![]).with_row_results(rows()));
        let executor = Executor::with_config(schema, adapter.clone(), with_response_bytes(5));

        let err = executor
            .execute_row_read(&qm, None, None, &cols(&["id", "name"]))
            .await
            .unwrap_err();

        match err {
            crate::FraiseQLError::ResponseTooLarge { bytes, limit } => {
                assert_eq!(bytes, 6, "`1` + `Alice`");
                assert_eq!(limit, 5);
            },
            other => panic!("expected ResponseTooLarge, got {other:?}"),
        }
    }

    /// Under it: served. Without this half the case above would pass against an arm
    /// that refused every read — the pair #1351's verification gate asks for.
    #[tokio::test]
    async fn a_row_read_under_the_response_byte_ceiling_is_read() {
        let schema = user_schema();
        let qm = match_on(&schema, "{ users { id name } }");
        let adapter = Arc::new(CapturingMockAdapter::new(vec![]).with_row_results(rows()));
        let executor = Executor::with_config(schema, adapter.clone(), with_response_bytes(6));

        let out = executor
            .execute_row_read(&qm, None, None, &cols(&["id", "name"]))
            .await
            .expect("exactly at the ceiling is under it");

        assert_eq!(out.rows.len(), 1);
    }

    /// **One budget for the whole stream, not one per frame.**
    ///
    /// Three 6-byte frames against a 12-byte ceiling: two are delivered and the third
    /// is refused. This is the case that makes the streaming wrap load-bearing — a
    /// budget rebuilt per frame would let all three through, because no single frame
    /// is over 12 bytes, and the response would be unbounded however many frames it
    /// had. That is the shape a stream is uniquely able to get wrong.
    #[tokio::test]
    async fn a_streamed_read_charges_one_budget_across_frames() {
        use futures::StreamExt as _;

        let schema = user_schema();
        let qm = match_on(&schema, "{ users { id name } }");
        let adapter = Arc::new(CapturingMockAdapter::new(vec![]).with_row_results(three_rows()));
        let executor = Executor::with_config(schema, adapter.clone(), with_response_bytes(12));

        let read = executor
            .stream_row_read(&qm, None, None, &cols(&["id", "name"]))
            .await
            .expect("the ceiling is charged per frame, so opening the stream succeeds");

        let mut stream = read.stream;
        let mut delivered = 0usize;
        let mut last = None;
        while let Some(frame) = stream.next().await {
            match frame {
                Ok(_) => delivered += 1,
                Err(e) => {
                    last = Some(e);
                    break;
                },
            }
        }

        assert_eq!(delivered, 2, "6 + 6 fits under 12; the third crosses it");
        match last {
            Some(crate::FraiseQLError::ResponseTooLarge { bytes, limit }) => {
                assert_eq!(bytes, 18, "the running total at refusal, not the frame size");
                assert_eq!(limit, 12);
            },
            other => panic!("expected the third frame to be refused, got {other:?}"),
        }
    }

    /// The whole stream fits: every frame is delivered and none errors.
    #[tokio::test]
    async fn a_streamed_read_within_the_ceiling_delivers_every_frame() {
        let schema = user_schema();
        let qm = match_on(&schema, "{ users { id name } }");
        let adapter = Arc::new(CapturingMockAdapter::new(vec![]).with_row_results(three_rows()));
        let executor = Executor::with_config(schema, adapter.clone(), with_response_bytes(18));

        let read = executor
            .stream_row_read(&qm, None, None, &cols(&["id", "name"]))
            .await
            .expect("under the ceiling");
        let frames = drain(read).await.expect("no frame errors");

        assert_eq!(frames.len(), 3);
    }

    /// The REST direct read — the JSON arm, and through it every embedded sub-read.
    ///
    /// `mock_user_results()` is `{"id":"1","name":"Alice"}` (26 bytes estimated) and
    /// `{"id":"2","name":"Bob"}` (24), so 50 together.
    #[tokio::test]
    async fn a_direct_json_read_over_the_response_byte_ceiling_is_refused() {
        let adapter = Arc::new(CapturingMockAdapter::new(mock_user_results()));
        let executor =
            Executor::with_config(test_schema(), adapter.clone(), with_response_bytes(40));
        let qm = crate::runtime::QueryMatcher::new(test_schema())
            .match_query("{ users { id name } }", None)
            .unwrap();

        let err = executor.execute_query_direct(&qm, None, None, None).await.unwrap_err();

        match err {
            crate::FraiseQLError::ResponseTooLarge { bytes, limit } => {
                assert_eq!(bytes, 50, "26 + 24");
                assert_eq!(limit, 40);
            },
            other => panic!("expected ResponseTooLarge, got {other:?}"),
        }
    }

    /// And its permitted twin.
    #[tokio::test]
    async fn a_direct_json_read_under_the_response_byte_ceiling_is_read() {
        let adapter = Arc::new(CapturingMockAdapter::new(mock_user_results()));
        let executor =
            Executor::with_config(test_schema(), adapter.clone(), with_response_bytes(50));
        let qm = crate::runtime::QueryMatcher::new(test_schema())
            .match_query("{ users { id name } }", None)
            .unwrap();

        executor
            .execute_query_direct(&qm, None, None, None)
            .await
            .expect("exactly at the ceiling");
    }

    /// The document path is charged too, so the same declared number means the same
    /// thing whether the request arrived as a GraphQL document or as a REST read.
    /// That parity is the whole point — the defect this closes was one ceiling
    /// meaning two different things depending on how the client connected.
    #[tokio::test]
    async fn a_document_read_over_the_response_byte_ceiling_is_refused() {
        let adapter = Arc::new(CapturingMockAdapter::new(mock_user_results()));
        let executor =
            Executor::with_config(test_schema(), adapter.clone(), with_response_bytes(40));

        let err = executor.execute("{ users { id name } }", None).await.unwrap_err();

        assert!(
            matches!(err, crate::FraiseQLError::ResponseTooLarge { .. }),
            "expected ResponseTooLarge, got {err:?}"
        );
    }

    /// The document twin under the ceiling.
    #[tokio::test]
    async fn a_document_read_under_the_response_byte_ceiling_is_read() {
        let adapter = Arc::new(CapturingMockAdapter::new(mock_user_results()));
        let executor =
            Executor::with_config(test_schema(), adapter.clone(), with_response_bytes(50));

        executor.execute("{ users { id name } }", None).await.expect("at the ceiling");
    }

    /// No ceiling declared, nothing charged — the control is the operator's
    /// declaration, not a default this change imposes on existing deployments.
    #[tokio::test]
    async fn a_deployment_with_no_declared_byte_ceiling_is_uncharged() {
        let schema = user_schema();
        let qm = match_on(&schema, "{ users { id name } }");
        let adapter = Arc::new(CapturingMockAdapter::new(vec![]).with_row_results(three_rows()));
        let executor = Executor::new(schema, adapter.clone());

        let out = executor
            .execute_row_read(&qm, None, None, &cols(&["id", "name"]))
            .await
            .expect("no ceiling declared, so nothing to exceed");

        assert_eq!(out.rows.len(), 3);
    }

    /// The ceiling is the compiled `[validation] max_response_bytes`, derived from the
    /// schema like every other schema-owned limit — not something only a programmatic
    /// embedder can set. Without this, the knob could be wired to nothing and every
    /// case above would still pass.
    #[test]
    fn the_ceiling_is_derived_from_the_compiled_schema() {
        let mut schema = user_schema();
        schema.validation_config = Some(crate::schema::ValidationConfig {
            max_response_bytes: Some(4_096),
            ..crate::schema::ValidationConfig::default()
        });

        let config = RuntimeConfig::default()
            .with_compiled_schema(&schema)
            .expect("the schema compiles");

        assert_eq!(config.max_response_bytes, Some(4_096));
    }

    // ---- a nested level named as a column ---------------------------------
    //
    // gRPC builds its columns from the type's scalar fields (`column_specs_from_type`
    // drops every object and list), so the transport never names a nested level. The
    // engine entry is public, though, and takes whatever columns it is handed: the
    // selection it classifies is those names, at the root, and nothing beneath them.

    /// `User` with a `Json` scalar and `team`, a nested `Team` whose `budget` masks.
    fn nested_schema() -> CompiledSchema {
        let mut schema = test_schema();
        let mut budget = FieldDefinition::new("budget", FieldType::Int);
        budget.requires_scope = Some("read:budget".to_string());
        budget.on_deny = FieldDenyPolicy::Mask;
        schema.types.push(TypeDefinition {
            fields: vec![FieldDefinition::new("id", FieldType::Id), budget],
            ..TypeDefinition::new("Team", "tb_teams")
        });
        schema.types.push(TypeDefinition {
            fields: vec![
                FieldDefinition::new("id", FieldType::Id),
                FieldDefinition::new("meta", FieldType::Json),
                FieldDefinition::new("team", FieldType::Object("Team".to_string())),
            ],
            ..TypeDefinition::new("User", "tb_users")
        });
        schema.security = Some(crate::schema::SecurityConfig::default());
        schema.build_indexes();
        schema
    }

    /// The match gRPC builds: the column names are the selection.
    fn column_match(schema: &CompiledSchema, fields: &[&str]) -> crate::runtime::QueryMatch {
        let query = schema.queries.iter().find(|q| q.name == "users").unwrap().clone();
        crate::runtime::QueryMatch::from_operation(
            query,
            fields.iter().map(ToString::to_string).collect(),
            HashMap::new(),
            schema.find_type("User"),
        )
        .unwrap()
    }

    fn json_cols(names: &[&str]) -> Vec<ColumnSpec> {
        names
            .iter()
            .map(|n| ColumnSpec {
                name:        (*n).to_string(),
                column_type: if *n == "id" {
                    RowViewColumnType::Text
                } else {
                    RowViewColumnType::Json
                },
            })
            .collect()
    }

    fn json_rows(value: &str) -> Vec<Vec<ColumnValue>> {
        vec![vec![
            ColumnValue::Text("1".into()),
            ColumnValue::Json(value.into()),
        ]]
    }

    /// Control: a `Json` scalar is a column like any other, and is served.
    #[tokio::test]
    async fn a_json_scalar_column_is_served_by_the_row_read() {
        let schema = nested_schema();
        let qm = column_match(&schema, &["id", "meta"]);
        let adapter =
            Arc::new(CapturingMockAdapter::new(vec![]).with_row_results(json_rows(r#"{"k":1}"#)));
        let executor = Executor::new(schema, adapter.clone());

        let out = executor
            .execute_row_read(&qm, None, Some(&principal()), &json_cols(&["id", "meta"]))
            .await
            .unwrap();

        assert_eq!(out.rows.len(), 1);
        assert_eq!(out.columns.iter().map(|c| c.name.as_str()).collect::<Vec<_>>(), ["id", "meta"]);
    }

    /// **Reproduction.** An object field named as a column is read as one: the stored
    /// `Team`, `budget` and all, reaches the caller with none of `Team`'s gates — a row
    /// has no key beneath a column to classify, mask or row-gate.
    #[tokio::test]
    async fn an_object_field_named_as_a_column_is_refused_by_the_row_read() {
        let schema = nested_schema();
        let qm = column_match(&schema, &["id", "team"]);
        let adapter = Arc::new(
            CapturingMockAdapter::new(vec![])
                .with_row_results(json_rows(r#"{"id":"7","budget":900}"#)),
        );
        let executor = Executor::new(schema, adapter.clone());

        let result = executor
            .execute_row_read(&qm, None, Some(&principal()), &json_cols(&["id", "team"]))
            .await;

        assert!(result.is_err(), "Team served as a column: {:?}", result.map(|r| r.rows));
        assert!(adapter.captured_row_read().is_none(), "nothing is read");
    }

    /// **Reproduction**, the streaming arm: the same column is streamed.
    #[tokio::test]
    async fn an_object_field_named_as_a_column_is_refused_by_the_streamed_row_read() {
        let schema = nested_schema();
        let qm = column_match(&schema, &["id", "team"]);
        let adapter = Arc::new(
            CapturingMockAdapter::new(vec![])
                .with_row_results(json_rows(r#"{"id":"7","budget":900}"#)),
        );
        let executor = Executor::new(schema, adapter);

        let opened = executor
            .stream_row_read(&qm, None, Some(&principal()), &json_cols(&["id", "team"]))
            .await;

        assert!(opened.is_err(), "Team streamed as a column");
    }
}

// ── mod enum_membership: the read path's call site is load-bearing (#1362) ────
//
// The write half is pinned next door in `runners/mutation/tests.rs`. This is the
// other call site: a read reaches its enums through the matcher, not through the
// mutation chokepoint, so removing either one leaves the other's tests green. The
// adapter here HAS rows to give, so a query that is not refused answers 200 with
// data — which is precisely what the defect did.
mod enum_membership {
    use super::*;
    use crate::schema::{
        ArgumentDefinition, EnumDefinition, EnumValueDefinition, InputFieldDefinition,
        InputObjectDefinition,
    };

    fn enum_arg(name: &str, type_name: &str) -> ArgumentDefinition {
        ArgumentDefinition {
            name:          name.to_string(),
            arg_type:      FieldType::Enum(type_name.to_string()),
            nullable:      true,
            default_value: None,
            description:   None,
            deprecation:   None,
            localized:     false,
        }
    }

    /// `orders(status: OrderStatus)` over an enum of three members.
    fn schema() -> CompiledSchema {
        let mut schema = test_schema();
        schema.enums.push(
            EnumDefinition::new("OrderStatus")
                .with_value(EnumValueDefinition::new("PENDING"))
                .with_value(EnumValueDefinition::new("SHIPPED"))
                .with_value(EnumValueDefinition::new("CANCELLED")),
        );
        schema.input_types.push(
            InputObjectDefinition::new("OrderProbeInput")
                .with_field(InputFieldDefinition::new("status", "OrderStatus")),
        );
        if let Some(users) = schema.queries.iter_mut().find(|q| q.name == "users") {
            users.arguments.push(enum_arg("status", "OrderStatus"));
        }
        // `test_schema` declares the query but no `User` type, and `order_by_inputs`
        // skips a return type it cannot adjudicate — so without this the `orderBy`
        // argument falls back to `JSON`, the walk never enters it, and the
        // `SortDirection` cases below would pass while proving nothing.
        schema.types.push(
            TypeDefinition::new("User", "v_user")
                .with_field(FieldDefinition::new("id", FieldType::Id))
                .with_field(FieldDefinition::new("name", FieldType::String)),
        );
        schema.build_indexes();
        schema
    }

    /// `SortDirection` is derived, not authored, and it is an enum like any other.
    #[test]
    fn the_derived_sort_direction_enum_is_in_scope() {
        let schema = schema();
        let def = schema.find_enum("SortDirection").expect("derived alongside OrderByInput");
        let members: Vec<&str> = def.values.iter().map(|v| v.name.as_str()).collect();
        assert_eq!(members, ["ASC", "DESC"], "the published members are upper case");
        assert!(
            schema.find_input_type("UserOrderByInput").is_some(),
            "the derived item type must exist or the orderBy cases below prove nothing"
        );
    }

    /// ⚠ **A deliberate break, pinned so it stays a decision.**
    ///
    /// `OrderByClause::from_graphql_json` upper-cases the written direction
    /// (`dir_str.to_ascii_uppercase()`), so `direction: "desc"` was honoured — while the
    /// schema published `SortDirection` with members `ASC` and `DESC`, and introspection,
    /// all four generated clients and the REST `?sort=-name` translation emit only those.
    /// The parser was quietly wider than the contract the schema advertises.
    ///
    /// #1362 makes the engine honour what it publishes, so this is now refused. The
    /// alternative was to carve `SortDirection` out of the enum rule, which is the kind of
    /// exception that rots — and which would have left the enum half of `orderBy`
    /// unvalidated, the very defect being fixed.
    #[tokio::test]
    async fn a_lower_case_sort_direction_is_now_refused() {
        let err = executor()
            .execute(
                r#"{ users(orderBy: [{field: "name", direction: "desc"}]) { id name } }"#,
                None,
            )
            .await
            .expect_err("`desc` is not a member of SortDirection; `DESC` is");
        assert!(
            err.to_string().contains("SortDirection"),
            "the refusal must name the enum so the migration is obvious: {err}"
        );
    }

    #[tokio::test]
    async fn the_published_spelling_of_a_sort_direction_is_served() {
        let result = executor()
            .execute(
                r#"{ users(orderBy: [{field: "name", direction: "DESC"}]) { id name } }"#,
                None,
            )
            .await
            .expect("`DESC` is exactly what the schema publishes");
        assert!(result.get("data").is_some(), "{result}");
    }

    /// The boundary, stated rather than discovered.
    ///
    /// `orderBy` has a second, *object* form — `{name: "desc"}`, a field-to-direction map —
    /// which `order_by_argument_type`'s own doc comment says "keeps executing but has no
    /// expression in this type". It carries neither `field` nor `direction`, so the declared
    /// `UserOrderByInput` cannot adjudicate it and the walk passes it through untouched,
    /// exactly as #939 requires. Lower case still works there.
    ///
    /// So the asymmetry is real and deliberate: the form the schema describes is checked,
    /// the form it does not describe is not. Pinned so nobody reads it as an oversight —
    /// and note the ARRAY form is a different thing again, where the parser itself demands
    /// a `field` key before this walk is ever consulted.
    #[tokio::test]
    async fn the_untyped_object_form_of_order_by_is_still_passed_through() {
        let result = executor()
            .execute(r#"{ users(orderBy: {name: "desc"}) { id name } }"#, None)
            .await
            .expect("a shape the derived input type does not describe is not adjudicated");
        assert!(result.get("data").is_some(), "{result}");
    }

    /// And the array form's own requirement is untouched: the parser demands `field`, and
    /// that refusal is its own, not this walk's. Without this the case above could be
    /// passing because `orderBy` stopped being adjudicated at all.
    #[tokio::test]
    async fn the_array_form_still_demands_a_field_key() {
        let err = executor()
            .execute(r#"{ users(orderBy: [{name: "desc"}]) { id name } }"#, None)
            .await
            .expect_err("an array item without `field` is refused by the orderBy parser");
        assert!(
            err.to_string().contains("missing 'field'"),
            "the refusal must be the parser's, not the enum walk's: {err}"
        );
    }

    fn executor() -> Executor {
        // Rows to give: an unrefused query answers with data, which is the shape
        // the defect had — a 200 carrying `BANANA` straight through to SQL.
        Executor::new(schema(), Arc::new(MockAdapter::new(mock_user_results())))
    }

    #[tokio::test]
    async fn an_inline_non_member_literal_is_refused_on_a_read() {
        let err = executor()
            .execute("{ users(status: BANANA) { id name } }", None)
            .await
            .expect_err("a value that is not a member of OrderStatus must not reach SQL");
        assert!(err.to_string().contains("OrderStatus"), "the refusal must name the enum: {err}");
    }

    #[tokio::test]
    async fn a_non_member_supplied_by_variable_is_refused_on_a_read() {
        let vars = serde_json::json!({"s": "BANANA"});
        let err = executor()
            .execute("query Q($s: OrderStatus) { users(status: $s) { id name } }", Some(&vars))
            .await
            .expect_err("a non-member supplied by variable must not reach SQL");
        assert!(err.to_string().contains("OrderStatus"), "the refusal must name the enum: {err}");
    }

    /// The third call site. A multi-root document does not reach the matcher's copy
    /// of this check — `field_selection_to_query` re-serialises each root into a
    /// synthetic document carrying no variable *declarations*, so the matcher sees an
    /// empty list and passes. Its own call in `execute_dispatch` is what adjudicates
    /// here, and without this case removing that line leaves every other enum test
    /// green.
    #[tokio::test]
    async fn a_non_member_variable_in_a_multi_root_document_is_refused() {
        let vars = serde_json::json!({"s": "BANANA"});
        let err = executor()
            .execute(
                "query Q($s: OrderStatus) { a: users(status: $s) { id } b: users { id } }",
                Some(&vars),
            )
            .await
            .expect_err("a multi-root document's variables must be adjudicated too");
        assert!(err.to_string().contains("OrderStatus"), "the refusal must name the enum: {err}");
    }

    /// The counterweight: a declared member is served, so this module cannot pass
    /// by refusing every read.
    #[tokio::test]
    async fn a_declared_member_is_still_served() {
        let result = executor()
            .execute("{ users(status: SHIPPED) { id name } }", None)
            .await
            .expect("a declared member must be served, not refused");
        assert!(result.get("data").is_some(), "a declared member must produce data: {result}");
    }
}
