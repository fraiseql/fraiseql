#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

use async_trait::async_trait;
use chrono::Utc;

use indexmap::IndexMap;
use super::*;
use crate::{
    db::{SupportsMutations, types::JsonbValue, types::sql_hints::OrderByClause, where_clause::WhereClause},
    runtime::{JsonbOptimizationOptions, JsonbStrategy, RuntimeConfig},
    schema::{
        AutoParams, CompiledSchema, CursorType, FieldDefinition, FieldDenyPolicy, FieldType,
        InjectedParamSource, QueryDefinition, RoleDefinition, SecurityConfig, TypeDefinition,
    },
    security::{DefaultRLSPolicy, SecurityContext},
};

// ── Shared test fixtures (accessible to all sub-modules via `use super::*`) ──

/// Capturing mock that records the WHERE clause and limit/offset it receives.
/// Used to verify parameter threading from executor to adapter.
struct CapturingMockAdapter {
    mock_results:   Vec<JsonbValue>,
    captured_where: std::sync::Mutex<Option<WhereClause>>,
    captured_limit: std::sync::Mutex<Option<u32>>,
    captured_offset: std::sync::Mutex<Option<u32>>,
}

impl CapturingMockAdapter {
    fn new(mock_results: Vec<JsonbValue>) -> Self {
        Self {
            mock_results,
            captured_where: std::sync::Mutex::new(None),
            captured_limit: std::sync::Mutex::new(None),
            captured_offset: std::sync::Mutex::new(None),
        }
    }

    fn captured_where(&self) -> Option<WhereClause> {
        self.captured_where.lock().unwrap().clone()
    }

    fn captured_limit(&self) -> Option<u32> {
        *self.captured_limit.lock().unwrap()
    }

    fn captured_offset(&self) -> Option<u32> {
        *self.captured_offset.lock().unwrap()
    }
}

// Reason: DatabaseAdapter is defined with #[async_trait]; all implementations must match
// async_trait: dyn-dispatch required; remove when RTN + Send is stable (RFC 3425)
#[async_trait]
impl DatabaseAdapter for CapturingMockAdapter {
    async fn execute_with_projection(
        &self,
        view: &str,
        _projection: Option<&crate::schema::SqlProjectionHint>,
        where_clause: Option<&WhereClause>,
        limit: Option<u32>,
        offset: Option<u32>,
        _order_by: Option<&[OrderByClause]>,
    ) -> Result<Vec<JsonbValue>> {
        self.execute_where_query(view, where_clause, limit, offset, None).await
    }

    async fn execute_where_query(
        &self,
        _view: &str,
        where_clause: Option<&WhereClause>,
        limit: Option<u32>,
        offset: Option<u32>,
        _order_by: Option<&[OrderByClause]>,
    ) -> Result<Vec<JsonbValue>> {
        *self.captured_where.lock().unwrap() = where_clause.cloned();
        *self.captured_limit.lock().unwrap() = limit;
        *self.captured_offset.lock().unwrap() = offset;
        Ok(self.mock_results.clone())
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

impl SupportsMutations for CapturingMockAdapter {}

/// Mock database adapter for testing.
///
/// Supports both uniform mode (same results for every view) and per-view mode
/// (`with_view()` builder) so tests can verify correct query routing.
struct MockAdapter {
    /// Default results returned for any view that has no specific override.
    mock_results:   Vec<JsonbValue>,
    /// Per-view result overrides. When present, `execute_where_query` returns
    /// these instead of `mock_results`, enabling routing-correctness tests.
    view_responses: std::collections::HashMap<String, Vec<JsonbValue>>,
}

impl MockAdapter {
    /// Uniform mode: all views return the same results.
    fn new(mock_results: Vec<JsonbValue>) -> Self {
        Self {
            mock_results,
            view_responses: std::collections::HashMap::new(),
        }
    }

    /// Per-view mode builder: register a specific result set for a named view.
    fn with_view(mut self, view: &str, results: Vec<JsonbValue>) -> Self {
        self.view_responses.insert(view.to_string(), results);
        self
    }
}

// Reason: DatabaseAdapter is defined with #[async_trait]; all implementations must match
// its transformed method signatures to satisfy the trait contract
// async_trait: dyn-dispatch required; remove when RTN + Send is stable (RFC 3425)
#[async_trait]
impl DatabaseAdapter for MockAdapter {
    async fn execute_with_projection(
        &self,
        view: &str,
        _projection: Option<&crate::schema::SqlProjectionHint>,
        where_clause: Option<&WhereClause>,
        limit: Option<u32>,
        _offset: Option<u32>,
        _order_by: Option<&[OrderByClause]>,
    ) -> Result<Vec<JsonbValue>> {
        // Fall back to standard query for tests
        self.execute_where_query(view, where_clause, limit, None, None).await
    }

    async fn execute_where_query(
        &self,
        view: &str,
        _where_clause: Option<&WhereClause>,
        _limit: Option<u32>,
        _offset: Option<u32>,
        _order_by: Option<&[OrderByClause]>,
    ) -> Result<Vec<JsonbValue>> {
        // Return per-view override if registered, otherwise fall back to uniform results.
        if let Some(results) = self.view_responses.get(view) {
            return Ok(results.clone());
        }
        Ok(self.mock_results.clone())
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

impl SupportsMutations for MockAdapter {}

/// Read-only adapter that returns false from `supports_mutations()` —
/// used to test the runtime mutation guard in `execute_mutation_query`.
struct ReadOnlyMockAdapter;

// Reason: DatabaseAdapter is defined with #[async_trait]; all implementations must match
// its transformed method signatures to satisfy the trait contract
// async_trait: dyn-dispatch required; remove when RTN + Send is stable (RFC 3425)
#[async_trait]
impl DatabaseAdapter for ReadOnlyMockAdapter {
    async fn execute_with_projection(
        &self,
        view: &str,
        _projection: Option<&crate::schema::SqlProjectionHint>,
        where_clause: Option<&WhereClause>,
        limit: Option<u32>,
        _offset: Option<u32>,
        _order_by: Option<&[OrderByClause]>,
    ) -> Result<Vec<JsonbValue>> {
        self.execute_where_query(view, where_clause, limit, None, None).await
    }

    async fn execute_where_query(
        &self,
        _view: &str,
        _where_clause: Option<&WhereClause>,
        _limit: Option<u32>,
        _offset: Option<u32>,
        _order_by: Option<&[OrderByClause]>,
    ) -> Result<Vec<JsonbValue>> {
        Ok(vec![])
    }

    async fn health_check(&self) -> Result<()> {
        Ok(())
    }

    fn database_type(&self) -> DatabaseType {
        DatabaseType::SQLite
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

    fn supports_mutations(&self) -> bool {
        false
    }
}

fn test_schema() -> CompiledSchema {
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
    });
    schema
}

fn mock_user_results() -> Vec<JsonbValue> {
    vec![
        JsonbValue::new(serde_json::json!({"id": "1", "name": "Alice"})),
        JsonbValue::new(serde_json::json!({"id": "2", "name": "Bob"})),
    ]
}

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

        assert!(result.contains("\"data\""));
        assert!(result.contains("\"users\""));
        assert!(result.contains("\"id\""));
        assert!(result.contains("\"name\""));
    }

    #[tokio::test]
    async fn test_execute_json() {
        let schema = test_schema();
        let adapter = Arc::new(MockAdapter::new(mock_user_results()));
        let executor = Executor::new(schema, adapter);

        let query = "{ users { id name } }";
        let result = executor.execute_json(query, None).await.unwrap();

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

        assert!(result.contains("__schema"));
        assert!(result.contains("Query"));
    }

    #[tokio::test]
    async fn test_introspection_type_query() {
        let schema = test_schema();
        let adapter = Arc::new(MockAdapter::new(vec![]));
        let executor = Executor::new(schema, adapter);

        let query = r#"{ __type(name: "Int") { kind name } }"#;
        let result = executor.execute(query, None).await.unwrap();

        assert!(result.contains("__type"));
        assert!(result.contains("Int"));
    }

    #[tokio::test]
    async fn test_introspection_unknown_type() {
        let schema = test_schema();
        let adapter = Arc::new(MockAdapter::new(vec![]));
        let executor = Executor::new(schema, adapter);

        let query = r#"{ __type(name: "UnknownType") { kind name } }"#;
        let result = executor.execute(query, None).await.unwrap();

        // Unknown type returns null
        assert!(result.contains("null"));
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
        assert_eq!(executor.classify_query(query).unwrap(), QueryType::NodeQuery);
    }

    #[test]
    fn test_classify_node_query_with_variable() {
        let schema = test_schema();
        let adapter = Arc::new(MockAdapter::new(vec![]));
        let executor = Executor::new(schema, adapter);

        let query = r"query GetNode($id: ID!) { node(id: $id) { id } }";
        assert_eq!(executor.classify_query(query).unwrap(), QueryType::NodeQuery);
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
        assert!(output.contains("__schema"));
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
            user_id:          user_id.to_string(),
            roles:            vec![],
            tenant_id:        tenant_id.map(str::to_string),
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

// ── mod mutation: mutation execution and adapter capability guard ─────────

mod mutation {
    use super::*;

    // Regression tests for issue #53 ──────────────────────────────────────
    //
    // The executor must fall back to operation.table when mutation_def.sql_source
    // is None.  Before the fix, the "has no sql_source configured" error was
    // returned unconditionally whenever sql_source was absent (e.g. when a schema
    // was compiled via the core Rust codegen path rather than the CLI converter).

    /// A mutation compiled without an explicit `sql_source` (only operation.table set)
    /// must NOT return a "has no `sql_source` configured" error.  Instead it should
    /// fall back to operation.table and attempt to call the SQL function, which in
    /// this test returns "function returned no rows" (the mock adapter is empty) —
    /// proving the executor reached the function-call stage (issue #53 regression).
    #[tokio::test]
    async fn test_mutation_falls_back_to_operation_table_when_sql_source_none() {
        use crate::schema::{MutationDefinition, MutationOperation};

        let mut schema = CompiledSchema::new();
        schema.mutations.push(MutationDefinition {
            name: "createUser".to_string(),
            return_type: "User".to_string(),
            // sql_source deliberately absent — simulates codegen path before the fix.
            sql_source: None,
            operation: MutationOperation::Insert {
                table: "fn_create_user".to_string(),
            },
            ..MutationDefinition::new("createUser", "User")
        });

        let adapter = Arc::new(MockAdapter::new(vec![]));
        let executor = Executor::new(schema, adapter);

        let err = executor.execute("mutation { createUser { id } }", None).await.unwrap_err();

        let msg = err.to_string();
        assert!(
            !msg.contains("has no sql_source configured"),
            "executor still failed on missing sql_source instead of using operation.table: {msg}"
        );
        assert!(
            msg.contains("function returned no rows") || msg.contains("no rows"),
            "expected 'no rows' error after fallback, got: {msg}"
        );
    }

    /// Mutations against a non-capable adapter must return `FraiseQLError::Validation`
    /// with a diagnostic message, not silently call `execute_function_call`.
    #[tokio::test]
    async fn test_mutation_rejected_by_non_capable_adapter() {
        use crate::schema::MutationDefinition;

        let mut schema = CompiledSchema::new();
        schema.mutations.push(MutationDefinition {
            sql_source: Some("fn_create_user".to_string()),
            ..MutationDefinition::new("createUser", "User")
        });

        let adapter = Arc::new(ReadOnlyMockAdapter);
        let executor = Executor::new(schema, adapter);

        let err = executor.execute("mutation { createUser { id } }", None).await.unwrap_err();

        let msg = err.to_string();
        assert!(
            msg.contains("does not support mutations"),
            "expected 'does not support mutations' diagnostic, got: {msg}"
        );
        assert!(msg.contains("createUser"), "error message should name the mutation, got: {msg}");
    }

    /// When both `sql_source` and operation.table are absent the executor must still
    /// return a clear validation error (not panic or silently succeed).
    #[tokio::test]
    async fn test_mutation_errors_when_both_sql_source_and_table_absent() {
        use crate::schema::{MutationDefinition, MutationOperation};

        let mut schema = CompiledSchema::new();
        schema.mutations.push(MutationDefinition {
            name: "deleteUser".to_string(),
            return_type: "User".to_string(),
            sql_source: None,
            // Custom operation has no table — no fallback available.
            operation: MutationOperation::Custom,
            ..MutationDefinition::new("deleteUser", "User")
        });

        let adapter = Arc::new(MockAdapter::new(vec![]));
        let executor = Executor::new(schema, adapter);

        let err = executor.execute("mutation { deleteUser { id } }", None).await.unwrap_err();

        assert!(
            err.to_string().contains("has no sql_source configured"),
            "expected sql_source error, got: {err}"
        );
    }

    // R9: SQLite/read-only adapter mutation guard — error type verification ─

    /// Mutations against a read-only adapter must return `FraiseQLError::Validation`
    /// specifically — not `FraiseQLError::Database` or `FraiseQLError::Internal`.
    /// This pins the error type so a future refactor cannot silently change it.
    #[tokio::test]
    async fn test_mutation_guard_returns_validation_error_not_database_or_internal() {
        use crate::schema::MutationDefinition;

        let mut schema = CompiledSchema::new();
        schema.mutations.push(MutationDefinition {
            sql_source: Some("fn_create_user".to_string()),
            ..MutationDefinition::new("createUser", "User")
        });

        let adapter = Arc::new(ReadOnlyMockAdapter);
        let executor = Executor::new(schema, adapter);

        let err = executor.execute("mutation { createUser { id } }", None).await.unwrap_err();

        // Must be Validation — not Internal, Database, or any other variant.
        assert!(
            matches!(err, FraiseQLError::Validation { .. }),
            "expected FraiseQLError::Validation for read-only adapter, got: {err:?}"
        );
    }

    /// The error message from the mutation guard must mention the mutation name
    /// so the caller can identify which mutation triggered the guard.
    #[tokio::test]
    async fn test_mutation_guard_error_message_is_actionable() {
        use crate::schema::MutationDefinition;

        let mut schema = CompiledSchema::new();
        schema.mutations.push(MutationDefinition {
            sql_source: Some("fn_delete_account".to_string()),
            ..MutationDefinition::new("deleteAccount", "User")
        });

        let adapter = Arc::new(ReadOnlyMockAdapter);
        let executor = Executor::new(schema, adapter);

        let err = executor.execute("mutation { deleteAccount { id } }", None).await.unwrap_err();

        let msg = err.to_string();
        assert!(
            msg.contains("deleteAccount"),
            "mutation guard message should name the mutation, got: {msg}"
        );
        assert!(
            msg.contains("mutation") || msg.contains("does not support"),
            "mutation guard message should explain the reason, got: {msg}"
        );
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

        let err = result.expect_err("alias limit must be enforced even when depth and complexity are disabled");
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
        validator
            .validate_query(query)
            .unwrap_or_else(|e| panic!("query within alias limit must pass when depth and complexity are disabled: {e:?}"));
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
        });

        let user_row = JsonbValue::new(serde_json::json!({"id": "1", "type": "user"}));
        let adapter = Arc::new(MockAdapter::new(vec![]).with_view("v_user", vec![user_row]));

        let executor = Executor::new(schema, adapter);
        let result = executor.execute("{ users { id type } }", None).await.unwrap();

        // v_user must return the user row, not the empty default.
        assert!(
            result.contains("\"type\":\"user\""),
            "expected user row from v_user, got: {result}"
        );
    }
}

// ── mod auto_params: has_where, has_limit, has_offset threading ──────────

mod auto_params {
    use super::*;

    fn schema_with_auto_params(auto_params: AutoParams) -> CompiledSchema {
        let mut schema = CompiledSchema::new();
        schema.queries.push(QueryDefinition {
            name:                "users".to_string(),
            return_type:         "User".to_string(),
            returns_list:        true,
            nullable:            false,
            arguments:           Vec::new(),
            sql_source:          Some("v_user".to_string()),
            description:         None,
            auto_params,
            deprecation:         None,
            jsonb_column:        "data".to_string(),
            relay:               false,
            relay_cursor_column: None,
            relay_cursor_type:   Default::default(),
            inject_params:       Default::default(),
            cache_ttl_seconds:   None,
            additional_views:    vec![],
            requires_role:       None,
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
    use super::*;
    use indexmap::IndexMap;

    fn schema_with_inject_params(inject_params: IndexMap<String, InjectedParamSource>) -> CompiledSchema {
        let mut schema = CompiledSchema::new();
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
                ..Default::default()
            },
            deprecation:         None,
            jsonb_column:        "data".to_string(),
            relay:               false,
            relay_cursor_column: None,
            relay_cursor_type:   Default::default(),
            inject_params,
            cache_ttl_seconds:   None,
            additional_views:    vec![],
            requires_role:       None,
        });
        schema
    }

    fn tenant_security_context() -> SecurityContext {
        SecurityContext {
            user_id:          "user-42".to_string(),
            roles:            vec!["viewer".to_string()],
            tenant_id:        Some("tenant-abc".to_string()),
            scopes:           vec!["read:User".to_string()],
            attributes:       Default::default(),
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
        let config = RuntimeConfig::default()
            .with_rls_policy(Arc::new(DefaultRLSPolicy::new()));
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

    /// C13: Verify RLS + inject_params compose into AND(rls, inject)
    #[tokio::test]
    async fn test_rls_and_inject_params_compose_into_and() {
        let mut inject = IndexMap::new();
        inject.insert("tenant_id".to_string(), InjectedParamSource::Jwt("tenant_id".to_string()));
        let schema = schema_with_inject_params(inject);
        let adapter = Arc::new(CapturingMockAdapter::new(mock_user_results()));
        let config = RuntimeConfig::default()
            .with_rls_policy(Arc::new(DefaultRLSPolicy::new()));
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
        let config = RuntimeConfig::default()
            .with_rls_policy(Arc::new(DefaultRLSPolicy::new()));
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
            auto_params:         Default::default(),
            deprecation:         None,
            jsonb_column:        "data".to_string(),
            relay:               false,
            relay_cursor_column: None,
            relay_cursor_type:   Default::default(),
            inject_params:       Default::default(),
            cache_ttl_seconds:   None,
            additional_views:    vec![],
            requires_role:       None,
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
            default_role: None,
            multi_tenant: false,
            additional:   Default::default(),
        });

        schema.types.push(user_type);
        schema
    }

    fn viewer_context() -> SecurityContext {
        SecurityContext {
            user_id:          "user-42".to_string(),
            roles:            vec!["viewer".to_string()],
            tenant_id:        None,
            scopes:           vec!["read:User".to_string()],
            attributes:       Default::default(),
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
            user_id:          "admin-1".to_string(),
            roles:            vec!["admin".to_string()],
            tenant_id:        None,
            scopes:           vec!["admin:*".to_string(), "read:User.email".to_string()],
            attributes:       Default::default(),
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
        let config = RuntimeConfig {
            ..RuntimeConfig::default()
        };
        let executor = Executor::with_config(schema, adapter, config);

        let ctx = viewer_context();
        let result = executor
            .execute_with_security("{ users { id salary } }", None, &ctx)
            .await;

        assert!(result.is_err(), "querying rejected field should fail");
        let err = result.unwrap_err();
        let err_msg = format!("{err}");
        assert!(
            err_msg.contains("salary") || err_msg.contains("authorization") || err_msg.contains("Authorization") || err_msg.contains("forbidden") || err_msg.contains("Forbidden"),
            "error should mention the forbidden field or authorization, got: {err_msg}"
        );
    }

    /// C16b: Querying a rejected field as admin succeeds
    #[tokio::test]
    async fn test_reject_field_allowed_for_admin() {
        let schema = schema_with_rbac_fields();
        let adapter = Arc::new(MockAdapter::new(mock_user_results()));
        let config = RuntimeConfig {
            ..RuntimeConfig::default()
        };
        let executor = Executor::with_config(schema, adapter, config);

        let ctx = admin_context();
        let result = executor
            .execute_with_security("{ users { id salary } }", None, &ctx)
            .await;

        assert!(result.is_ok(), "admin should be able to query rejected field: {:?}", result.err());
    }

    /// C17: Querying a masked field as unauthorized user returns null
    #[tokio::test]
    async fn test_mask_field_returns_null_for_unauthorized() {
        let schema = schema_with_rbac_fields();
        let results = vec![JsonbValue::new(serde_json::json!({"id": 1, "name": "Alice", "email": "alice@example.com"}))];
        let adapter = Arc::new(MockAdapter::new(results));
        let config = RuntimeConfig {
            ..RuntimeConfig::default()
        };
        let executor = Executor::with_config(schema, adapter, config);

        let ctx = viewer_context();
        let result = executor
            .execute_with_security("{ users { id email } }", None, &ctx)
            .await
            .unwrap();

        // Parse response JSON to verify masking
        let response: serde_json::Value = serde_json::from_str(&result).unwrap();
        let users = &response["data"]["users"];
        assert!(users.is_array(), "expected users array in response: {result}");
        for user in users.as_array().unwrap() {
            assert!(
                user["email"].is_null(),
                "masked field 'email' should be null for unauthorized user, got: {}",
                user["email"]
            );
            // id should still have real value
            assert!(
                !user["id"].is_null(),
                "unmasked field 'id' should have real value"
            );
        }
    }

    /// C17b: Querying a masked field as authorized user returns real value
    #[tokio::test]
    async fn test_mask_field_returns_real_value_for_authorized() {
        let schema = schema_with_rbac_fields();
        let results = vec![JsonbValue::new(serde_json::json!({"id": 1, "name": "Alice", "email": "alice@example.com"}))];
        let adapter = Arc::new(MockAdapter::new(results));
        let config = RuntimeConfig {
            ..RuntimeConfig::default()
        };
        let executor = Executor::with_config(schema, adapter, config);

        let ctx = admin_context();
        let result = executor
            .execute_with_security("{ users { id email } }", None, &ctx)
            .await
            .unwrap();

        let response: serde_json::Value = serde_json::from_str(&result).unwrap();
        let users = &response["data"]["users"];
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
        let config = RuntimeConfig {
            ..RuntimeConfig::default()
        };
        let executor = Executor::with_config(schema, adapter, config);

        let ctx = viewer_context();
        let result = executor
            .execute_with_security("{ users { id name } }", None, &ctx)
            .await;

        assert!(result.is_ok(), "public fields should always be accessible: {:?}", result.err());
    }
}
