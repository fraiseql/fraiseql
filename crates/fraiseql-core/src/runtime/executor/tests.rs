#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

use std::collections::HashMap;

use chrono::Utc;
use indexmap::IndexMap;

use super::*;
use super::test_support::*;
use crate::{
    db::types::JsonbValue,
    runtime::{JsonbOptimizationOptions, JsonbStrategy, RuntimeConfig},
    schema::{
        AutoParams, CompiledSchema, CursorType, FieldDefinition, FieldDenyPolicy, FieldType,
        InjectedParamSource, QueryDefinition, RoleDefinition, SecurityConfig, TenancyConfig,
        TypeDefinition,
    },
    security::SecurityContext,
};

// ── mod query: basic query execution ─────────────────────────────────────

mod query {
    use super::*;

    #[tokio::test]
    async fn test_executor_new() {
        let schema = test_schema();
        let adapter = Arc::new(MockAdapter::new(vec![]));
        let executor = Executor::new(schema, adapter);
        assert_eq!(executor.schema().queries.len(), 1);
    }

    #[tokio::test]
    async fn test_execute_query() {
        let schema = test_schema();
        let adapter = Arc::new(MockAdapter::new(mock_user_results()));
        let executor = Executor::new(schema, adapter);

        let query = "{ users { id name } }";
        let result = executor.execute(query, None).await.unwrap();

        assert!(result.get("data").is_some());
        assert!(result["data"].get("users").is_some());
        assert!(result["data"]["users"][0].get("id").is_some());
        assert!(result["data"]["users"][0].get("name").is_some());
    }

    #[tokio::test]
    async fn test_execute_json() {
        let schema = test_schema();
        let adapter = Arc::new(MockAdapter::new(mock_user_results()));
        let executor = Executor::new(schema, adapter);

        let query = "{ users { id name } }";
        let result = executor.execute(query, None).await.unwrap();

        assert!(result.get("data").is_some());
        assert!(result["data"].get("users").is_some());
    }

    #[tokio::test]
    async fn test_executor_with_config() {
        let schema = test_schema();
        let adapter = Arc::new(MockAdapter::new(vec![]));
        let config = RuntimeConfig {
            cache_query_plans:    false,
            max_query_depth:      5,
            max_query_complexity: 500,
            enable_tracing:       true,
            field_filter:         None,
            rls_policy:           None,
            query_timeout_ms:     30_000,
            jsonb_optimization:   JsonbOptimizationOptions::default(),
            query_validation:     None,
            audit_mutations:      false,
        };
        let executor = Executor::with_config(schema, adapter, config);

        assert!(!executor.config().cache_query_plans);
        assert_eq!(executor.config().max_query_depth, 5);
        assert!(executor.config().enable_tracing);
    }
}

// ── mod introspection: __schema and __type queries ────────────────────────

mod introspection {
    use super::*;

    #[tokio::test]
    async fn test_introspection_schema_query() {
        let schema = test_schema();
        let adapter = Arc::new(MockAdapter::new(vec![]));
        let executor = Executor::new(schema, adapter);

        let query = r"{ __schema { queryType { name } } }";
        let result = executor.execute(query, None).await.unwrap();

        assert!(result["data"].get("__schema").is_some());
    }

    #[tokio::test]
    async fn test_introspection_type_query() {
        let schema = test_schema();
        let adapter = Arc::new(MockAdapter::new(vec![]));
        let executor = Executor::new(schema, adapter);

        let query = r#"{ __type(name: "Int") { kind name } }"#;
        let result = executor.execute(query, None).await.unwrap();

        assert!(result["data"].get("__type").is_some());
        assert_eq!(result["data"]["__type"]["name"], "Int");
    }

    #[tokio::test]
    async fn test_introspection_unknown_type() {
        let schema = test_schema();
        let adapter = Arc::new(MockAdapter::new(vec![]));
        let executor = Executor::new(schema, adapter);

        let query = r#"{ __type(name: "UnknownType") { kind name } }"#;
        let result = executor.execute(query, None).await.unwrap();

        // Unknown type returns null
        assert!(result["data"]["__type"].is_null());
    }
}

// ── mod classify: query type detection ───────────────────────────────────

mod classify {
    use super::*;

    #[test]
    fn test_detect_introspection_schema() {
        let schema = test_schema();
        let adapter = Arc::new(MockAdapter::new(vec![]));
        let executor = Executor::new(schema, adapter);

        let query = r"{ __schema { types { name } } }";
        assert_eq!(executor.classify_query(query).unwrap(), QueryType::IntrospectionSchema);
    }

    #[test]
    fn test_detect_introspection_type() {
        let schema = test_schema();
        let adapter = Arc::new(MockAdapter::new(vec![]));
        let executor = Executor::new(schema, adapter);

        let query = r#"{ __type(name: "User") { fields { name } } }"#;
        assert_eq!(
            executor.classify_query(query).unwrap(),
            QueryType::IntrospectionType("User".to_string()),
        );
    }

    #[test]
    fn test_classify_node_query_inline_id() {
        let schema = test_schema();
        let adapter = Arc::new(MockAdapter::new(vec![]));
        let executor = Executor::new(schema, adapter);

        let query = r#"{ node(id: "VXNlcjoxMjM=") { ... on User { name } } }"#;
        assert!(matches!(executor.classify_query(query).unwrap(), QueryType::NodeQuery { .. }));
    }

    #[test]
    fn test_classify_node_query_with_variable() {
        let schema = test_schema();
        let adapter = Arc::new(MockAdapter::new(vec![]));
        let executor = Executor::new(schema, adapter);

        let query = r"query GetNode($id: ID!) { node(id: $id) { id } }";
        assert!(matches!(executor.classify_query(query).unwrap(), QueryType::NodeQuery { .. }));
    }

    #[test]
    fn test_classify_node_query_extracts_inline_fragment_selections() {
        let schema = test_schema();
        let adapter = Arc::new(MockAdapter::new(vec![]));
        let executor = Executor::new(schema, adapter);

        let query = r#"{ node(id: "VXNlcjoxMjM=") { ... on User { name email } } }"#;
        let qt = executor.classify_query(query).unwrap();
        match qt {
            QueryType::NodeQuery { selections } => {
                let names: Vec<&str> = selections.iter().map(|s| s.name.as_str()).collect();
                assert_eq!(names, vec!["name", "email"]);
            },
            other => panic!("expected NodeQuery, got {other:?}"),
        }
    }

    #[test]
    fn test_classify_node_query_direct_fields_without_fragment() {
        let schema = test_schema();
        let adapter = Arc::new(MockAdapter::new(vec![]));
        let executor = Executor::new(schema, adapter);

        let query = r"query GetNode($id: ID!) { node(id: $id) { id name } }";
        let qt = executor.classify_query(query).unwrap();
        match qt {
            QueryType::NodeQuery { selections } => {
                let names: Vec<&str> = selections.iter().map(|s| s.name.as_str()).collect();
                assert_eq!(names, vec!["id", "name"]);
            },
            other => panic!("expected NodeQuery, got {other:?}"),
        }
    }

    #[test]
    fn test_classify_node_query_rejects_substring_match() {
        let schema = test_schema();
        let adapter = Arc::new(MockAdapter::new(vec![]));
        let executor = Executor::new(schema, adapter);

        // "nodeCounts" contains "node(" as a substring — must NOT match
        let query = r#"{ nodeCounts(id: "x") { total } }"#;
        assert_eq!(executor.classify_query(query).unwrap(), QueryType::Regular);
    }

    #[test]
    fn test_classify_introspection_type_extracts_name() {
        let schema = test_schema();
        let adapter = Arc::new(MockAdapter::new(vec![]));
        let executor = Executor::new(schema, adapter);

        // Standard double-quoted argument
        let q = r#"{ __type(name: "User") { name } }"#;
        assert_eq!(
            executor.classify_query(q).unwrap(),
            QueryType::IntrospectionType("User".to_string()),
        );

        // No space after colon
        let q2 = r#"{ __type(name:"Query") { name } }"#;
        assert_eq!(
            executor.classify_query(q2).unwrap(),
            QueryType::IntrospectionType("Query".to_string()),
        );
    }

    #[test]
    fn test_classify_no_false_positive_schema_in_comment() {
        let schema = test_schema();
        let adapter = Arc::new(MockAdapter::new(vec![]));
        let executor = Executor::new(schema, adapter);

        // __schema appears in a comment — should classify as Regular, not introspection.
        let q = "{ users { id } } # compare against __schema response";
        assert_eq!(executor.classify_query(q).unwrap(), QueryType::Regular);
    }

    #[test]
    fn test_classify_no_false_positive_service_in_argument() {
        let schema = test_schema();
        let adapter = Arc::new(MockAdapter::new(vec![]));
        let executor = Executor::new(schema, adapter);

        // "_service" appears as a string argument — must NOT route to federation.
        let q = r#"{ search(query: "_service_name") { id } }"#;
        assert_eq!(executor.classify_query(q).unwrap(), QueryType::Regular);
    }

    #[test]
    fn test_classify_no_false_positive_entities_alias() {
        let schema = test_schema();
        let adapter = Arc::new(MockAdapter::new(vec![]));
        let executor = Executor::new(schema, adapter);

        // "_entities" used as an alias — the actual field is "users", not _entities.
        // Must NOT route to federation.
        let q = r"{ _entities: users { id } }";
        assert_eq!(executor.classify_query(q).unwrap(), QueryType::Regular);
    }

    #[test]
    fn test_classify_federation_service() {
        let schema = test_schema();
        let adapter = Arc::new(MockAdapter::new(vec![]));
        let executor = Executor::new(schema, adapter);

        let q = r"{ _service { sdl } }";
        assert_eq!(
            executor.classify_query(q).unwrap(),
            QueryType::Federation("_service".to_string()),
        );
    }

    #[test]
    fn test_classify_federation_entities() {
        let schema = test_schema();
        let adapter = Arc::new(MockAdapter::new(vec![]));
        let executor = Executor::new(schema, adapter);

        let q = r#"{ _entities(representations: [{ __typename: "User", id: "1" }]) { ... on User { name } } }"#;
        assert_eq!(
            executor.classify_query(q).unwrap(),
            QueryType::Federation("_entities".to_string()),
        );
    }
}

// ── mod context: ExecutionContext lifecycle ───────────────────────────────

mod context {
    use super::*;

    #[test]
    fn test_execution_context_creation() {
        let ctx = ExecutionContext::new("query-123".to_string());
        assert_eq!(ctx.query_id(), "query-123");
        assert!(!ctx.is_cancelled());
    }

    #[test]
    fn test_execution_context_cancellation_token() {
        let ctx = ExecutionContext::new("query-456".to_string());
        let token = ctx.cancellation_token();
        assert!(!token.is_cancelled());

        // Cancel the token
        token.cancel();
        assert!(token.is_cancelled());
        assert!(ctx.is_cancelled());
    }

    #[tokio::test]
    async fn test_execute_with_context_success() {
        let schema = test_schema();
        let adapter = Arc::new(MockAdapter::new(vec![]));
        let executor = Executor::new(schema, adapter);

        let ctx = ExecutionContext::new("test-query-1".to_string());
        let query = r"{ __schema { queryType { name } } }";

        let result = executor.execute_with_context(query, None, &ctx).await;
        let output = result.unwrap_or_else(|e| panic!("expected Ok for execute_with_context: {e}"));
        assert!(output["data"].get("__schema").is_some());
    }

    #[tokio::test]
    async fn test_execute_with_context_already_cancelled() {
        let schema = test_schema();
        let adapter = Arc::new(MockAdapter::new(vec![]));
        let executor = Executor::new(schema, adapter);

        let ctx = ExecutionContext::new("test-query-2".to_string());
        let token = ctx.cancellation_token().clone();

        // Cancel before execution
        token.cancel();

        let query = r"{ __schema { queryType { name } } }";
        let result = executor.execute_with_context(query, None, &ctx).await;

        let err = result.expect_err("expected Err for already-cancelled context");
        match err {
            FraiseQLError::Cancelled { query_id, reason } => {
                assert_eq!(query_id, "test-query-2");
                assert!(reason.contains("before execution"));
            },
            e => panic!("Expected Cancelled error, got: {}", e),
        }
    }

    #[tokio::test]
    async fn test_execute_with_context_cancelled_during_execution() {
        let schema = test_schema();
        let adapter = Arc::new(MockAdapter::new(vec![]));
        let executor = Executor::new(schema, adapter);

        let ctx = ExecutionContext::new("test-query-3".to_string());
        let token = ctx.cancellation_token().clone();

        // Spawn a task to cancel after a short delay
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(10)).await;
            token.cancel();
        });

        let query = r"{ __schema { queryType { name } } }";
        let result = executor.execute_with_context(query, None, &ctx).await;

        // Depending on timing, may succeed or be cancelled (both are acceptable)
        // But if cancelled, it should be our error
        if let Err(FraiseQLError::Cancelled { query_id, .. }) = result {
            assert_eq!(query_id, "test-query-3");
        }
    }

    #[test]
    fn test_execution_context_clone() {
        let ctx = ExecutionContext::new("query-clone".to_string());
        let ctx_clone = ctx.clone();

        assert_eq!(ctx.query_id(), ctx_clone.query_id());
        assert!(!ctx_clone.is_cancelled());

        // Cancel original
        ctx.cancellation_token().cancel();

        // Clone should also see cancellation (same token)
        assert!(ctx_clone.is_cancelled());
    }

    #[test]
    fn test_error_cancelled_constructor() {
        let err = FraiseQLError::cancelled("query-001", "user requested cancellation");

        assert!(err.to_string().contains("Query cancelled"));
        assert_eq!(err.status_code(), 408);
        assert_eq!(err.error_code(), "CANCELLED");
        assert!(err.is_retryable());
        assert!(err.is_server_error());
    }
}

// ── mod config: RuntimeConfig and JSONB optimization ─────────────────────

mod config {
    use super::*;

    #[test]
    fn test_jsonb_strategy_in_runtime_config() {
        let config = RuntimeConfig {
            cache_query_plans:    false,
            max_query_depth:      5,
            max_query_complexity: 500,
            enable_tracing:       true,
            field_filter:         None,
            rls_policy:           None,
            query_timeout_ms:     30_000,
            jsonb_optimization:   JsonbOptimizationOptions::default(),
            query_validation:     None,
            audit_mutations:      false,
        };

        assert_eq!(config.jsonb_optimization.default_strategy, JsonbStrategy::Project);
        assert_eq!(config.jsonb_optimization.auto_threshold_percent, 80);
    }

    #[test]
    fn test_jsonb_strategy_custom_config() {
        let custom_options = JsonbOptimizationOptions {
            default_strategy:       JsonbStrategy::Stream,
            auto_threshold_percent: 50,
        };

        let config = RuntimeConfig {
            cache_query_plans:    false,
            max_query_depth:      5,
            max_query_complexity: 500,
            enable_tracing:       true,
            field_filter:         None,
            rls_policy:           None,
            query_timeout_ms:     30_000,
            jsonb_optimization:   custom_options,
            query_validation:     None,
            audit_mutations:      false,
        };

        assert_eq!(config.jsonb_optimization.default_strategy, JsonbStrategy::Stream);
        assert_eq!(config.jsonb_optimization.auto_threshold_percent, 50);
    }
}

// ── mod inject: @inject parameter resolution (JWT claims → query params) ──

mod inject {
    use super::*;

    fn make_security_ctx(
        user_id: &str,
        tenant_id: Option<&str>,
        extra: &[(&str, serde_json::Value)],
    ) -> SecurityContext {
        use chrono::Utc;
        let now = Utc::now();
        SecurityContext {
            user_id:          crate::types::UserId::new(user_id),
            roles:            vec![],
            tenant_id:        tenant_id.map(crate::types::TenantId::new),
            scopes:           vec![],
            attributes:       extra.iter().map(|(k, v)| ((*k).to_string(), v.clone())).collect(),
            request_id:       "test-req".to_string(),
            ip_address:       None,
            authenticated_at: now,
            expires_at:       now + chrono::Duration::hours(1),
            issuer:           None,
            audience:         None,
        }
    }

    #[test]
    fn test_resolve_inject_sub_maps_to_user_id() {
        let ctx = make_security_ctx("user-42", None, &[]);
        let source = InjectedParamSource::Jwt("sub".to_string());
        let result = resolve_inject_value("user_id", &source, &ctx).unwrap();
        assert_eq!(result, serde_json::Value::String("user-42".to_string()));
    }

    #[test]
    fn test_resolve_inject_tenant_id_claim() {
        let ctx = make_security_ctx("user-1", Some("tenant-abc"), &[]);
        let source = InjectedParamSource::Jwt("tenant_id".to_string());
        let result = resolve_inject_value("tenant_id", &source, &ctx).unwrap();
        assert_eq!(result, serde_json::Value::String("tenant-abc".to_string()));
    }

    #[test]
    fn test_resolve_inject_org_id_alias() {
        let ctx = make_security_ctx("user-1", Some("org-xyz"), &[]);
        let source = InjectedParamSource::Jwt("org_id".to_string());
        let result = resolve_inject_value("org_id", &source, &ctx).unwrap();
        assert_eq!(result, serde_json::Value::String("org-xyz".to_string()));
    }

    #[test]
    fn test_resolve_inject_custom_attribute() {
        let ctx =
            make_security_ctx("user-1", None, &[("department", serde_json::json!("engineering"))]);
        let source = InjectedParamSource::Jwt("department".to_string());
        let result = resolve_inject_value("dept", &source, &ctx).unwrap();
        assert_eq!(result, serde_json::Value::String("engineering".to_string()));
    }

    #[test]
    fn test_resolve_inject_missing_claim_returns_error() {
        let ctx = make_security_ctx("user-1", None, &[]);
        let source = InjectedParamSource::Jwt("org_id".to_string());
        let err = resolve_inject_value("org_id", &source, &ctx).unwrap_err();
        assert!(matches!(err, FraiseQLError::Validation { .. }));
        let msg = err.to_string();
        assert!(msg.contains("org_id"), "Error should mention claim name");
    }

    #[test]
    fn test_resolve_inject_missing_tenant_id_returns_error() {
        let ctx = make_security_ctx("user-1", None, &[]);
        let source = InjectedParamSource::Jwt("tenant_id".to_string());
        let err = resolve_inject_value("tenant_id", &source, &ctx).unwrap_err();
        assert!(matches!(err, FraiseQLError::Validation { .. }));
    }

    #[tokio::test]
    async fn test_query_with_inject_rejects_unauthenticated() {
        let mut schema = test_schema();
        let mut inject_params = IndexMap::new();
        inject_params.insert("org_id".to_string(), InjectedParamSource::Jwt("org_id".to_string()));
        schema.queries.push(QueryDefinition {
            name: "org_items".to_string(),
            return_type: "User".to_string(),
            returns_list: true,
            nullable: false,
            arguments: Vec::new(),
            sql_source: Some("v_org_items".to_string()),
            description: None,
            auto_params: AutoParams::default(),
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
        let adapter = Arc::new(MockAdapter::new(vec![]));
        let executor = Executor::new(schema, adapter);

        // Execute without security context — should fail with Validation error
        let result = executor.execute("{ org_items { id } }", None).await;
        let err = result.expect_err("Expected Err for unauthenticated inject query");
        assert!(
            matches!(err, FraiseQLError::Validation { .. }),
            "Expected Validation error, got: {err:?}"
        );
    }
}

// ── mod masking: null_masked_fields ──────────────────────────────────────

mod masking {
    use super::*;

    #[test]
    fn test_null_masked_fields_object() {
        let mut value = serde_json::json!({"id": 1, "email": "alice@example.com", "name": "Alice"});
        null_masked_fields(&mut value, &["email".to_string()]);
        assert_eq!(value, serde_json::json!({"id": 1, "email": null, "name": "Alice"}));
    }

    #[test]
    fn test_null_masked_fields_array() {
        let mut value = serde_json::json!([
            {"id": 1, "email": "a@b.com", "salary": 100_000},
            {"id": 2, "email": "c@d.com", "salary": 120_000},
        ]);
        null_masked_fields(&mut value, &["email".to_string(), "salary".to_string()]);
        assert_eq!(
            value,
            serde_json::json!([
                {"id": 1, "email": null, "salary": null},
                {"id": 2, "email": null, "salary": null},
            ])
        );
    }

    #[test]
    fn test_null_masked_fields_no_masked() {
        let mut value = serde_json::json!({"id": 1, "name": "Alice"});
        let original = value.clone();
        null_masked_fields(&mut value, &[]);
        assert_eq!(value, original);
    }
}

// ── mod planning: query plan generation ──────────────────────────────────

mod planning {
    use super::*;

    #[test]
    fn test_plan_query_regular() {
        let schema = test_schema();
        let adapter = Arc::new(MockAdapter::new(vec![]));
        let executor = Executor::new(schema, adapter);

        let plan = executor.plan_query("{ users { id name } }", None).unwrap();
        assert_eq!(plan.query_type, "regular");
        assert!(plan.sql.contains("v_user"));
        assert_eq!(plan.views_accessed, vec!["v_user"]);
        assert!(plan.estimated_cost > 0);
    }

    #[test]
    fn test_plan_query_introspection() {
        let schema = test_schema();
        let adapter = Arc::new(MockAdapter::new(vec![]));
        let executor = Executor::new(schema, adapter);

        let plan = executor.plan_query("{ __schema { types { name } } }", None).unwrap();
        assert_eq!(plan.query_type, "introspection");
        assert!(plan.sql.is_empty());
        assert!(plan.views_accessed.is_empty());
    }

    #[test]
    fn test_plan_query_empty_rejected() {
        let schema = test_schema();
        let adapter = Arc::new(MockAdapter::new(vec![]));
        let executor = Executor::new(schema, adapter);

        let result = executor.plan_query("", None);
        assert!(result.is_err(), "expected Err for empty query, got: {result:?}");
    }
}

// ── mod security: DoS protection (alias / depth / complexity limits) ──────

mod security {
    // R10: Alias limit enforced independently of depth/complexity flags ─────

    /// When both depth and complexity validation are disabled, the alias limit
    /// must still be enforced. This tests that the alias check is NOT inside
    /// a depth/complexity gate and will catch alias amplification attacks even
    /// when other limits are turned off.
    #[test]
    fn test_alias_limit_enforced_when_depth_and_complexity_disabled() {
        use crate::graphql::complexity::{ComplexityValidationError, RequestValidator};

        let validator = RequestValidator::new()
            .with_depth_validation(false)
            .with_complexity_validation(false)
            .with_max_aliases(2);

        // 3 aliases — exceeds limit of 2.
        let query = "query { a: users { id } b: users { id } c: users { id } }";
        let result = validator.validate_query(query);

        let err = result
            .expect_err("alias limit must be enforced even when depth and complexity are disabled");
        assert!(
            matches!(
                err,
                ComplexityValidationError::TooManyAliases {
                    actual_aliases: 3,
                    ..
                }
            ),
            "error must be TooManyAliases with actual_aliases = 3, got: {err:?}"
        );
    }

    /// When aliases are within the limit, the query must pass even with other
    /// limits disabled — verifying that alias-disable=false doesn't block valid queries.
    #[test]
    fn test_alias_within_limit_passes_when_depth_and_complexity_disabled() {
        use crate::graphql::complexity::RequestValidator;

        let validator = RequestValidator::new()
            .with_depth_validation(false)
            .with_complexity_validation(false)
            .with_max_aliases(5);

        // 2 aliases — within limit of 5.
        let query = "query { a: users { id } b: users { id } }";
        validator.validate_query(query).unwrap_or_else(|e| {
            panic!(
                "query within alias limit must pass when depth and complexity are disabled: {e:?}"
            )
        });
    }
}

// ── mod field_rbac: C16+C17 — RBAC reject/mask through executor ──────────

mod field_rbac {
    use super::*;

    fn schema_with_rbac_fields() -> CompiledSchema {
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
        let mut user_type = TypeDefinition::new("User", "v_user");
        user_type.fields = vec![
            FieldDefinition {
                name:           "id".into(),
                field_type:     FieldType::Int,
                nullable:       false,
                description:    None,
                default_value:  None,
                vector_config:  None,
                alias:          None,
                deprecation:    None,
                requires_scope: None,
                on_deny:        FieldDenyPolicy::Reject,
                encryption:     None,
            },
            FieldDefinition {
                name:           "name".into(),
                field_type:     FieldType::String,
                nullable:       false,
                description:    None,
                default_value:  None,
                vector_config:  None,
                alias:          None,
                deprecation:    None,
                requires_scope: None,
                on_deny:        FieldDenyPolicy::Reject,
                encryption:     None,
            },
            // Protected field: reject when unauthorized
            FieldDefinition {
                name:           "salary".into(),
                field_type:     FieldType::Int,
                nullable:       true,
                description:    None,
                default_value:  None,
                vector_config:  None,
                alias:          None,
                deprecation:    None,
                requires_scope: Some("admin:*".to_string()),
                on_deny:        FieldDenyPolicy::Reject,
                encryption:     None,
            },
            // Protected field: mask when unauthorized
            FieldDefinition {
                name:           "email".into(),
                field_type:     FieldType::String,
                nullable:       true,
                description:    None,
                default_value:  None,
                vector_config:  None,
                alias:          None,
                deprecation:    None,
                requires_scope: Some("read:User.email".to_string()),
                on_deny:        FieldDenyPolicy::Mask,
                encryption:     None,
            },
        ];

        // Set up security config with role definitions for scope-based RBAC
        schema.security = Some(SecurityConfig {
            role_definitions: vec![
                RoleDefinition {
                    name:        "viewer".into(),
                    description: None,
                    scopes:      vec!["read:User".into()],
                },
                RoleDefinition {
                    name:        "admin".into(),
                    description: None,
                    scopes:      vec!["admin:*".into(), "read:User.email".into()],
                },
            ],
            default_role:     None,
            multi_tenant:     false,
            tenancy:          TenancyConfig::default(),
            additional:       HashMap::default(),
        });

        schema.types.push(user_type);
        schema
    }

    fn viewer_context() -> SecurityContext {
        SecurityContext {
            user_id:          "user-42".into(),
            roles:            vec!["viewer".to_string()],
            tenant_id:        None,
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

    fn admin_context() -> SecurityContext {
        SecurityContext {
            user_id:          "admin-1".into(),
            roles:            vec!["admin".to_string()],
            tenant_id:        None,
            scopes:           vec!["admin:*".to_string(), "read:User.email".to_string()],
            attributes:       HashMap::default(),
            request_id:       "req-002".to_string(),
            ip_address:       None,
            expires_at:       Utc::now() + chrono::Duration::hours(1),
            authenticated_at: Utc::now(),
            issuer:           None,
            audience:         None,
        }
    }

    /// C16: Querying a rejected field as unauthorized user returns Authorization error
    #[tokio::test]
    async fn test_reject_field_returns_authorization_error() {
        let schema = schema_with_rbac_fields();
        let adapter = Arc::new(MockAdapter::new(mock_user_results()));
        let config = RuntimeConfig::default();
        let executor = Executor::with_config(schema, adapter, config);

        let ctx = viewer_context();
        let result = executor.execute_with_security("{ users { id salary } }", None, &ctx).await;

        assert!(result.is_err(), "querying rejected field should fail");
        let err = result.unwrap_err();
        let err_msg = format!("{err}");
        assert!(
            err_msg.contains("salary")
                || err_msg.contains("authorization")
                || err_msg.contains("Authorization")
                || err_msg.contains("forbidden")
                || err_msg.contains("Forbidden"),
            "error should mention the forbidden field or authorization, got: {err_msg}"
        );
    }

    /// C16b: Querying a rejected field as admin succeeds
    #[tokio::test]
    async fn test_reject_field_allowed_for_admin() {
        let schema = schema_with_rbac_fields();
        let adapter = Arc::new(MockAdapter::new(mock_user_results()));
        let config = RuntimeConfig::default();
        let executor = Executor::with_config(schema, adapter, config);

        let ctx = admin_context();
        let result = executor.execute_with_security("{ users { id salary } }", None, &ctx).await;

        assert!(
            result.is_ok(),
            "admin should be able to query rejected field: {:?}",
            result.err()
        );
    }

    /// C17: Querying a masked field as unauthorized user returns null
    #[tokio::test]
    async fn test_mask_field_returns_null_for_unauthorized() {
        let schema = schema_with_rbac_fields();
        let results = vec![JsonbValue::new(
            serde_json::json!({"id": 1, "name": "Alice", "email": "alice@example.com"}),
        )];
        let adapter = Arc::new(MockAdapter::new(results));
        let config = RuntimeConfig::default();
        let executor = Executor::with_config(schema, adapter, config);

        let ctx = viewer_context();
        let result = executor
            .execute_with_security("{ users { id email } }", None, &ctx)
            .await
            .unwrap();

        // Verify masking using Value directly
        let users = &result["data"]["users"];
        assert!(users.is_array(), "expected users array in response: {result}");
        for user in users.as_array().unwrap() {
            assert!(
                user["email"].is_null(),
                "masked field 'email' should be null for unauthorized user, got: {}",
                user["email"]
            );
            // id should still have real value
            assert!(!user["id"].is_null(), "unmasked field 'id' should have real value");
        }
    }

    /// C17b: Querying a masked field as authorized user returns real value
    #[tokio::test]
    async fn test_mask_field_returns_real_value_for_authorized() {
        let schema = schema_with_rbac_fields();
        let results = vec![JsonbValue::new(
            serde_json::json!({"id": 1, "name": "Alice", "email": "alice@example.com"}),
        )];
        let adapter = Arc::new(MockAdapter::new(results));
        let config = RuntimeConfig::default();
        let executor = Executor::with_config(schema, adapter, config);

        let ctx = admin_context();
        let result = executor
            .execute_with_security("{ users { id email } }", None, &ctx)
            .await
            .unwrap();

        let users = &result["data"]["users"];
        for user in users.as_array().unwrap() {
            assert_eq!(
                user["email"], "alice@example.com",
                "authorized user should see real email value"
            );
        }
    }

    /// C16+C17: Public fields always accessible
    #[tokio::test]
    async fn test_public_fields_always_accessible() {
        let schema = schema_with_rbac_fields();
        let adapter = Arc::new(MockAdapter::new(mock_user_results()));
        let config = RuntimeConfig::default();
        let executor = Executor::with_config(schema, adapter, config);

        let ctx = viewer_context();
        let result = executor.execute_with_security("{ users { id name } }", None, &ctx).await;

        assert!(result.is_ok(), "public fields should always be accessible: {:?}", result.err());
    }
}

// ── mod executor_paths: H4 — requires_role anti-enumeration tests ─────────

mod executor_paths {
    use super::*;

    /// H4: `requires_role` returns "not found" (anti-enumeration), not "forbidden"
    #[tokio::test]
    async fn test_requires_role_returns_not_found_not_forbidden() {
        let mut schema = test_schema();
        schema.queries[0].requires_role = Some("admin".to_string());
        let adapter = Arc::new(MockAdapter::new(mock_user_results()));
        let executor = Executor::new(schema, adapter);

        // No security context at all → should say "not found"
        let result = executor.execute("{ users { id } }", None).await;
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("not found in schema"),
            "requires_role should produce 'not found', not 'forbidden', got: {err}"
        );
        assert!(
            !err.contains("forbidden") && !err.contains("Forbidden"),
            "must not reveal the query exists behind a role gate, got: {err}"
        );
    }

    /// H4: `requires_role` with wrong role still returns "not found"
    #[tokio::test]
    async fn test_requires_role_wrong_role_returns_not_found() {
        let mut schema = test_schema();
        schema.queries[0].requires_role = Some("admin".to_string());
        let adapter = Arc::new(MockAdapter::new(mock_user_results()));
        let executor = Executor::new(schema, adapter);

        let ctx = SecurityContext {
            user_id:          "user-42".into(),
            roles:            vec!["viewer".to_string()],
            tenant_id:        None,
            scopes:           vec![],
            attributes:       HashMap::default(),
            request_id:       "req-001".to_string(),
            ip_address:       None,
            expires_at:       Utc::now() + chrono::Duration::hours(1),
            authenticated_at: Utc::now(),
            issuer:           None,
            audience:         None,
        };
        let result = executor.execute_with_security("{ users { id } }", None, &ctx).await;
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("not found in schema"),
            "wrong role should produce 'not found', got: {err}"
        );
    }

    /// H4: `requires_role` with correct role succeeds
    #[tokio::test]
    async fn test_requires_role_correct_role_succeeds() {
        let mut schema = test_schema();
        schema.queries[0].requires_role = Some("admin".to_string());
        let adapter = Arc::new(MockAdapter::new(mock_user_results()));
        let executor = Executor::new(schema, adapter);

        let ctx = SecurityContext {
            user_id:          "admin-1".into(),
            roles:            vec!["admin".to_string()],
            tenant_id:        None,
            scopes:           vec![],
            attributes:       HashMap::default(),
            request_id:       "req-002".to_string(),
            ip_address:       None,
            expires_at:       Utc::now() + chrono::Duration::hours(1),
            authenticated_at: Utc::now(),
            issuer:           None,
            audience:         None,
        };
        let result = executor.execute_with_security("{ users { id } }", None, &ctx).await;
        assert!(result.is_ok(), "correct role should succeed: {:?}", result.err());
    }
}

// ── mod parse_cache: AST cache behaviour ─────────────────────────────────

mod parse_cache {
    use super::*;

    #[tokio::test]
    async fn test_cache_empty_before_first_execute() {
        let schema = test_schema();
        let adapter = Arc::new(MockAdapter::new(mock_user_results()));
        let executor = Executor::new(schema, adapter);

        assert_eq!(executor.parse_cache_entry_count(), 0, "cache must be empty before any call");
    }

    #[tokio::test]
    async fn test_cache_populated_after_first_execute() {
        let schema = test_schema();
        let adapter = Arc::new(MockAdapter::new(mock_user_results()));
        let executor = Executor::new(schema, adapter);

        executor.execute("{ users { id name } }", None).await.unwrap();

        // moka may apply a brief maintenance delay; run_pending_tasks() drains it.
        executor.ctx.parse_cache.run_pending_tasks();
        assert_eq!(
            executor.parse_cache_entry_count(),
            1,
            "one distinct query must produce exactly one cache entry"
        );
    }

    #[tokio::test]
    async fn test_cache_no_double_insert_for_repeated_query() {
        let schema = test_schema();
        let adapter = Arc::new(MockAdapter::new(mock_user_results()));
        let executor = Executor::new(schema, adapter);

        let query = "{ users { id name } }";
        executor.execute(query, None).await.unwrap();
        executor.execute(query, None).await.unwrap();
        executor.execute(query, None).await.unwrap();

        executor.ctx.parse_cache.run_pending_tasks();
        assert_eq!(
            executor.parse_cache_entry_count(),
            1,
            "repeating the same query must not grow the cache beyond 1 entry"
        );
    }

    #[tokio::test]
    async fn test_cache_separate_entries_for_distinct_queries() {
        let schema = test_schema();
        let adapter = Arc::new(MockAdapter::new(mock_user_results()));
        let executor = Executor::new(schema, adapter);

        executor.execute("{ users { id name } }", None).await.unwrap();
        executor.execute("{ users { id } }", None).await.unwrap();

        executor.ctx.parse_cache.run_pending_tasks();
        assert_eq!(
            executor.parse_cache_entry_count(),
            2,
            "two distinct query strings must produce two cache entries"
        );
    }
}
