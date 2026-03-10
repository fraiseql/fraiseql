#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

use super::*;
use async_trait::async_trait;
use crate::{
    db::{MutationCapable, types::JsonbValue, where_clause::WhereClause},
    runtime::{JsonbOptimizationOptions, JsonbStrategy},
    schema::{AutoParams, CompiledSchema, QueryDefinition},
};

/// Mock database adapter for testing.
struct MockAdapter {
    mock_results: Vec<JsonbValue>,
}

impl MockAdapter {
    fn new(mock_results: Vec<JsonbValue>) -> Self {
        Self { mock_results }
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
    ) -> Result<Vec<JsonbValue>> {
        // Fall back to standard query for tests
        self.execute_where_query(view, where_clause, limit, None).await
    }

    async fn execute_where_query(
        &self,
        _view: &str,
        _where_clause: Option<&WhereClause>,
        _limit: Option<u32>,
        _offset: Option<u32>,
    ) -> Result<Vec<JsonbValue>> {
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
        // Mock implementation: return empty results
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

impl MutationCapable for MockAdapter {}

/// Read-only adapter that returns false from supports_mutations() —
/// used to test the runtime mutation guard in execute_mutation_query.
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
    ) -> Result<Vec<JsonbValue>> {
        self.execute_where_query(view, where_clause, limit, None).await
    }

    async fn execute_where_query(
        &self,
        _view: &str,
        _where_clause: Option<&WhereClause>,
        _limit: Option<u32>,
        _offset: Option<u32>,
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
        name:         "users".to_string(),
        return_type:  "User".to_string(),
        returns_list: true,
        nullable:     false,
        arguments:    Vec::new(),
        sql_source:   Some("v_user".to_string()),
        description:  None,
        auto_params:  AutoParams::default(),
        deprecation:  None,
        jsonb_column: "data".to_string(),
        relay: false,
        relay_cursor_column: None,
        relay_cursor_type: Default::default(),
        inject_params:     Default::default(),
        cache_ttl_seconds:   None,
        additional_views: vec![],
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
    };

    let executor = Executor::with_config(schema, adapter, config);

    assert!(!executor.config().cache_query_plans);
    assert_eq!(executor.config().max_query_depth, 5);
    assert!(executor.config().enable_tracing);
}

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

#[test]
fn test_detect_introspection_schema() {
    let schema = test_schema();
    let adapter = Arc::new(MockAdapter::new(vec![]));
    let executor = Executor::new(schema, adapter);

    let query = r"{ __schema { types { name } } }";
    let query_type = executor.classify_query(query).unwrap();
    assert_eq!(query_type, QueryType::IntrospectionSchema);
}

#[test]
fn test_detect_introspection_type() {
    let schema = test_schema();
    let adapter = Arc::new(MockAdapter::new(vec![]));
    let executor = Executor::new(schema, adapter);

    let query = r#"{ __type(name: "User") { fields { name } } }"#;
    let query_type = executor.classify_query(query).unwrap();
    assert_eq!(query_type, QueryType::IntrospectionType("User".to_string()));
}

#[test]
fn test_classify_node_query_inline_id() {
    let schema = test_schema();
    let adapter = Arc::new(MockAdapter::new(vec![]));
    let executor = Executor::new(schema, adapter);

    let query = r#"{ node(id: "VXNlcjoxMjM=") { ... on User { name } } }"#;
    let query_type = executor.classify_query(query).unwrap();
    assert_eq!(query_type, QueryType::NodeQuery);
}

#[test]
fn test_classify_node_query_with_variable() {
    let schema = test_schema();
    let adapter = Arc::new(MockAdapter::new(vec![]));
    let executor = Executor::new(schema, adapter);

    let query = r"query GetNode($id: ID!) { node(id: $id) { id } }";
    let query_type = executor.classify_query(query).unwrap();
    assert_eq!(query_type, QueryType::NodeQuery);
}

#[test]
fn test_classify_node_query_rejects_substring_match() {
    let schema = test_schema();
    let adapter = Arc::new(MockAdapter::new(vec![]));
    let executor = Executor::new(schema, adapter);

    // "nodeCounts" contains "node(" as a substring — must NOT match
    let query = r#"{ nodeCounts(id: "x") { total } }"#;
    let query_type = executor.classify_query(query).unwrap();
    assert_eq!(query_type, QueryType::Regular);
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

// ==================== ExecutionContext Tests ====================

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
    assert!(result.is_ok());
    assert!(result.unwrap().contains("__schema"));
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

    assert!(result.is_err());
    match result.unwrap_err() {
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

// ========================================================================

// ========================================================================

#[test]
fn test_jsonb_strategy_in_runtime_config() {
    // Verify that RuntimeConfig includes JSONB optimization options
    let config = RuntimeConfig {
        cache_query_plans:    false,
        max_query_depth:      5,
        max_query_complexity: 500,
        enable_tracing:       true,
        field_filter:         None,
        rls_policy:           None,
        query_timeout_ms:     30_000,
        jsonb_optimization:   JsonbOptimizationOptions::default(),
    };

    assert_eq!(config.jsonb_optimization.default_strategy, JsonbStrategy::Project);
    assert_eq!(config.jsonb_optimization.auto_threshold_percent, 80);
}

#[test]
fn test_jsonb_strategy_custom_config() {
    // Verify custom JSONB strategy options in config
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
    };

    assert_eq!(config.jsonb_optimization.default_strategy, JsonbStrategy::Stream);
    assert_eq!(config.jsonb_optimization.auto_threshold_percent, 50);
}

// =========================================================================
// resolve_inject_value unit tests
// =========================================================================

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
    let ctx = make_security_ctx(
        "user-1",
        None,
        &[("department", serde_json::json!("engineering"))],
    );
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
    use indexmap::IndexMap;

    let mut schema = test_schema();
    // Add a query that requires inject
    let mut inject_params = IndexMap::new();
    inject_params.insert("org_id".to_string(), InjectedParamSource::Jwt("org_id".to_string()));
    schema.queries.push(QueryDefinition {
        name:                "org_items".to_string(),
        return_type:         "User".to_string(),
        returns_list:        true,
        nullable:            false,
        arguments:           Vec::new(),
        sql_source:          Some("v_org_items".to_string()),
        description:         None,
        auto_params:         AutoParams::default(),
        deprecation:         None,
        jsonb_column:        "data".to_string(),
        relay:               false,
        relay_cursor_column: None,
        relay_cursor_type:   Default::default(),
        inject_params,
        cache_ttl_seconds:   None,
        additional_views: vec![],
        requires_role:       None,
    });
    let adapter = Arc::new(MockAdapter::new(vec![]));
    let executor = Executor::new(schema, adapter);

    // Execute without security context — should fail with Validation error
    let result = executor.execute("{ org_items { id } }", None).await;
    assert!(result.is_err(), "Expected Err for unauthenticated inject query");
    let err = result.unwrap_err();
    assert!(
        matches!(err, FraiseQLError::Validation { .. }),
        "Expected Validation error, got: {err:?}"
    );
}

// =========================================================================
// null_masked_fields tests
// =========================================================================

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

    let plan = executor
        .plan_query("{ __schema { types { name } } }", None)
        .unwrap();
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
    assert!(result.is_err());
}

// ── Regression tests for issue #53 ──────────────────────────────────────
//
// The executor must fall back to operation.table when mutation_def.sql_source
// is None.  Before the fix, the "has no sql_source configured" error was
// returned unconditionally whenever sql_source was absent (e.g. when a schema
// was compiled via the core Rust codegen path rather than the CLI converter).

/// A mutation compiled without an explicit sql_source (only operation.table set)
/// must NOT return a "has no sql_source configured" error.  Instead it should
/// fall back to operation.table and attempt to call the SQL function, which in
/// this test returns "function returned no rows" (the mock adapter is empty) —
/// proving the executor reached the function-call stage (issue #53 regression).
#[tokio::test]
async fn test_mutation_falls_back_to_operation_table_when_sql_source_none() {
    use crate::schema::{MutationDefinition, MutationOperation};

    let mut schema = CompiledSchema::new();
    schema.mutations.push(MutationDefinition {
        name:       "createUser".to_string(),
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

    let err = executor
        .execute("mutation { createUser { id } }", None)
        .await
        .unwrap_err();

    let msg = err.to_string();
    // Must NOT be the "missing sql_source" error — the fallback must have fired.
    assert!(
        !msg.contains("has no sql_source configured"),
        "executor still failed on missing sql_source instead of using operation.table: {msg}"
    );
    // Must be the downstream "no rows" error — proving the SQL call was attempted.
    assert!(
        msg.contains("function returned no rows") || msg.contains("no rows"),
        "expected 'no rows' error after fallback, got: {msg}"
    );
}

/// Mutations against a non-capable adapter must return FraiseQLError::Validation
/// with a diagnostic message, not silently call execute_function_call.
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

    let err = executor
        .execute("mutation { createUser { id } }", None)
        .await
        .unwrap_err();

    let msg = err.to_string();
    assert!(
        msg.contains("does not support mutations"),
        "expected 'does not support mutations' diagnostic, got: {msg}"
    );
    assert!(
        msg.contains("createUser"),
        "error message should name the mutation, got: {msg}"
    );
}

/// When both sql_source and operation.table are absent the executor must still
/// return a clear validation error (not panic or silently succeed).
#[tokio::test]
async fn test_mutation_errors_when_both_sql_source_and_table_absent() {
    use crate::schema::{MutationDefinition, MutationOperation};

    let mut schema = CompiledSchema::new();
    schema.mutations.push(MutationDefinition {
        name:       "deleteUser".to_string(),
        return_type: "User".to_string(),
        sql_source: None,
        // Custom operation has no table — no fallback available.
        operation: MutationOperation::Custom,
        ..MutationDefinition::new("deleteUser", "User")
    });

    let adapter = Arc::new(MockAdapter::new(vec![]));
    let executor = Executor::new(schema, adapter);

    let err = executor
        .execute("mutation { deleteUser { id } }", None)
        .await
        .unwrap_err();

    assert!(
        err.to_string().contains("has no sql_source configured"),
        "expected sql_source error, got: {err}"
    );
}
