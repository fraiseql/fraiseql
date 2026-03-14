#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};

use async_trait::async_trait;

use super::*;
use crate::{
    compiler::aggregation::OrderByClause,
    db::{
        CursorValue, MutationCapable, RelayDatabaseAdapter,
        traits::{RelayPageResult},
        types::JsonbValue,
        where_clause::WhereClause,
    },
    graphql::FieldSelection,
    runtime::{JsonbOptimizationOptions, JsonbStrategy},
    schema::{AutoParams, CompiledSchema, CursorType, QueryDefinition},
};

// ── selections_contain_field — mutation-targeted tests (C1–C6) ───────────────
//
// These tests target the private `selections_contain_field` function in `query.rs`.
// They are placed inline (not in an external test file) because the function is private.
//
// | Mutant | What cargo-mutants changes | Killed by |
// |--------|---------------------------|-----------|
// | C1 | Replace body with `true` | `absent_field_not_found_in_non_empty_list` |
// | C2 | Replace body with `false` | `present_field_found_in_single_element_list` |
// | C3 | `==` → `!=` in name check | `present_field_found_in_single_element_list` |
// | C4 | Delete inline-fragment branch | `field_found_inside_inline_fragment` |
// | C5 | `starts_with("...")` → `true` | `absent_field_not_found_in_non_empty_list` |
// | C6 | Final `false` → `true` | `absent_field_not_found_in_non_empty_list` |

fn make_field(name: &str) -> FieldSelection {
    FieldSelection {
        name:         name.to_string(),
        alias:        None,
        arguments:    vec![],
        nested_fields: vec![],
        directives:   vec![],
    }
}

fn make_inline_fragment(inner_fields: Vec<FieldSelection>) -> FieldSelection {
    // Inline fragments are represented as FieldSelection with name starting with "..."
    FieldSelection {
        name:         "...on UserConnection".to_string(),
        alias:        None,
        arguments:    vec![],
        nested_fields: inner_fields,
        directives:   vec![],
    }
}

/// C1/C3: Field is found when it's the only element in the list.
#[test]
fn present_field_found_in_single_element_list() {
    let selections = vec![make_field("totalCount")];
    assert!(
        super::query::selections_contain_field(&selections, "totalCount"),
        "C1/C3: 'totalCount' must be found in single-element list"
    );
}

/// C2: Field is not found in a non-empty list that doesn't contain it.
#[test]
fn absent_field_not_found_in_non_empty_list() {
    let selections = vec![make_field("edges"), make_field("pageInfo")];
    assert!(
        !super::query::selections_contain_field(&selections, "totalCount"),
        "C2/C5/C6: 'totalCount' must not be found when absent"
    );
}

/// C3b: Different field name must not match.
#[test]
fn field_name_must_match_exactly() {
    let selections = vec![make_field("totalCountXYZ"), make_field("total")];
    assert!(
        !super::query::selections_contain_field(&selections, "totalCount"),
        "C3: partial name match must not count as a match"
    );
}

/// C4: Field is found inside an inline fragment (name starts with "...").
#[test]
fn field_found_inside_inline_fragment() {
    let inner = vec![make_field("totalCount"), make_field("edges")];
    let selections = vec![make_field("pageInfo"), make_inline_fragment(inner)];
    assert!(
        super::query::selections_contain_field(&selections, "totalCount"),
        "C4: 'totalCount' must be found inside an inline fragment"
    );
}

/// C4b: Field not in the inline fragment is not returned as found.
#[test]
fn absent_field_not_found_inside_inline_fragment() {
    let inner = vec![make_field("edges"), make_field("pageInfo")];
    let selections = vec![make_inline_fragment(inner)];
    assert!(
        !super::query::selections_contain_field(&selections, "totalCount"),
        "C4b: 'totalCount' must not be found if absent from inline fragment"
    );
}

/// Empty selection list always returns false.
#[test]
fn empty_selections_returns_false() {
    assert!(
        !super::query::selections_contain_field(&[], "totalCount"),
        "C6: empty selections must return false"
    );
}

/// Multiple candidates: returns true when any matches.
#[test]
fn field_found_among_multiple_selections() {
    let selections = vec![
        make_field("edges"),
        make_field("pageInfo"),
        make_field("totalCount"),
    ];
    assert!(
        super::query::selections_contain_field(&selections, "totalCount"),
        "must find totalCount in multi-element list"
    );
}

/// Mock database adapter for testing.
struct MockAdapter {
    mock_results: Vec<JsonbValue>,
}

impl MockAdapter {
    fn new(mock_results: Vec<JsonbValue>) -> Self {
        Self { mock_results }
    }
}

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

    async fn execute_function_call(
        &self,
        _function_name: &str,
        _args: &[serde_json::Value],
    ) -> Result<Vec<std::collections::HashMap<String, serde_json::Value>>> {
        Ok(vec![])
    }
}

impl MutationCapable for MockAdapter {}

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
        relay_cursor_type:   Default::default(),
        inject_params:       Default::default(),
        cache_ttl_seconds:   None,
        additional_views:    vec![],
        requires_role:       None,
        rest:                None,
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
    use indexmap::IndexMap;

    let mut schema = test_schema();
    // Add a query that requires inject
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
        relay_cursor_type: Default::default(),
        inject_params,
        cache_ttl_seconds: None,
        additional_views: vec![],
        requires_role: None,
        rest: None,
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

/// When both sql_source and operation.table are absent the executor must still
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

// ── execute_with_security — mutation-targeted tests (D1–D6) ──────────────────
//
// These tests target mutations in execute_regular_query_with_security (query.rs).
//
// | Mutant | Location | What cargo-mutants changes | Killed by |
// |--------|----------|---------------------------|-----------|
// | D1 | line 47 | delete ! in role check    | user_with_role_can_access_role_gated_query |
// | D2 | line 47 | delete ! in role check    | user_without_role_is_rejected_for_role_gated_query |
// | D3 | line 35 | is_expired() not checked  | expired_token_is_rejected |
// | D4 | line 47 | == with != in any() check | user_with_role_can_access_role_gated_query |

fn make_role_gated_schema(required_role: &str) -> CompiledSchema {
    let mut schema = CompiledSchema::new();
    schema.queries.push(QueryDefinition {
        name:                "adminReport".to_string(),
        return_type:         "Report".to_string(),
        returns_list:        true,
        nullable:            false,
        arguments:           Vec::new(),
        sql_source:          Some("v_admin_report".to_string()),
        description:         None,
        auto_params:         AutoParams::default(),
        deprecation:         None,
        jsonb_column:        "data".to_string(),
        relay:               false,
        relay_cursor_column: None,
        relay_cursor_type:   Default::default(),
        inject_params:       Default::default(),
        cache_ttl_seconds:   None,
        additional_views:    vec![],
        requires_role:       Some(required_role.to_string()),
        rest:                None,
    });
    schema
}

fn make_security_ctx_with_roles(user_id: &str, roles: Vec<String>) -> SecurityContext {
    use chrono::Utc;
    let now = Utc::now();
    SecurityContext {
        user_id:          user_id.to_string(),
        roles,
        tenant_id:        None,
        scopes:           vec![],
        attributes:       std::collections::HashMap::new(),
        request_id:       "test-req".to_string(),
        ip_address:       None,
        authenticated_at: now,
        expires_at:       now + chrono::Duration::hours(1),
        issuer:           None,
        audience:         None,
    }
}

fn make_expired_security_ctx() -> SecurityContext {
    use chrono::Utc;
    let now = Utc::now();
    SecurityContext {
        user_id:          "user-1".to_string(),
        roles:            vec![],
        tenant_id:        None,
        scopes:           vec![],
        attributes:       std::collections::HashMap::new(),
        request_id:       "test-req".to_string(),
        ip_address:       None,
        authenticated_at: now - chrono::Duration::hours(2),
        expires_at:       now - chrono::Duration::hours(1), // expired 1h ago
        issuer:           None,
        audience:         None,
    }
}

/// D1/D4: User WITH the required role must be allowed to execute the query.
///
/// Mutation "delete !" at line 47 → `if security_context.roles.iter().any(|r| r == required_role)`
/// would REJECT users who have the role (inverts the check).
/// Mutation "== with !=" → always rejects matching roles.
#[tokio::test]
async fn user_with_role_can_access_role_gated_query() {
    let schema = make_role_gated_schema("admin");
    let adapter = Arc::new(MockAdapter::new(vec![
        JsonbValue::new(serde_json::json!({"id": "1", "title": "Report"})),
    ]));
    let executor = Executor::new(schema, adapter);
    let ctx = make_security_ctx_with_roles("user-1", vec!["admin".to_string()]);

    let result = executor
        .execute_with_security("query { adminReport { id title } }", None, &ctx)
        .await;
    assert!(
        result.is_ok(),
        "D1/D4: user with required role must succeed, got: {:?}",
        result.err()
    );
}

/// D2: User WITHOUT the required role must be rejected.
///
/// This is the baseline correctness test. With the mutant (delete !), a user
/// WITHOUT the role would be ALLOWED — this test would catch that.
#[tokio::test]
async fn user_without_role_is_rejected_for_role_gated_query() {
    let schema = make_role_gated_schema("admin");
    let adapter = Arc::new(MockAdapter::new(vec![]));
    let executor = Executor::new(schema, adapter);
    let ctx = make_security_ctx_with_roles("user-1", vec!["viewer".to_string()]);

    let err = executor
        .execute_with_security("query { adminReport { id title } }", None, &ctx)
        .await
        .unwrap_err();
    // Must fail with a "not found" error (not "forbidden") to prevent enumeration
    let msg = err.to_string();
    assert!(
        msg.contains("not found"),
        "D2: unauthorized access must return 'not found', got: {msg}"
    );
}

/// D2b: User with NO roles at all must be rejected.
#[tokio::test]
async fn user_with_no_roles_is_rejected_for_role_gated_query() {
    let schema = make_role_gated_schema("admin");
    let adapter = Arc::new(MockAdapter::new(vec![]));
    let executor = Executor::new(schema, adapter);
    let ctx = make_security_ctx_with_roles("user-1", vec![]);

    let result =
        executor.execute_with_security("query { adminReport { id title } }", None, &ctx).await;
    assert!(result.is_err(), "D2b: user with no roles must be rejected");
}

/// D3: Expired security token must be rejected before any query execution.
///
/// Mutation "replace body with Ok(String::new())" or "delete is_expired() check"
/// would bypass expiration validation, allowing expired tokens.
#[tokio::test]
async fn expired_token_is_rejected() {
    let schema = test_schema();
    let adapter = Arc::new(MockAdapter::new(vec![]));
    let executor = Executor::new(schema, adapter);
    let ctx = make_expired_security_ctx();

    let err = executor
        .execute_with_security("query { users { id } }", None, &ctx)
        .await
        .unwrap_err();
    let msg = err.to_string();
    assert!(
        msg.contains("expired") || msg.contains("Expired"),
        "D3: expired token must fail with 'expired' message, got: {msg}"
    );
}

/// D5: Non-role-gated query must be accessible to any authenticated user.
///
/// Verifies that the role check is only applied when requires_role is Some.
#[tokio::test]
async fn non_role_gated_query_is_accessible_without_any_role() {
    let schema = test_schema(); // "users" query has requires_role: None
    let adapter = Arc::new(MockAdapter::new(vec![
        JsonbValue::new(serde_json::json!({"id": "1", "name": "Alice"})),
    ]));
    let executor = Executor::new(schema, adapter);
    let ctx = make_security_ctx_with_roles("user-1", vec![]);

    let result = executor.execute_with_security("query { users { id } }", None, &ctx).await;
    assert!(
        result.is_ok(),
        "D5: query without requires_role must succeed for any authenticated user, got: {:?}",
        result.err()
    );
}

// ── E/F: Projection hint — mutation-targeted tests ────────────────────────────
//
// These tests verify that `execute_regular_query` and
// `execute_regular_query_with_security` correctly compute the SqlProjectionHint.
//
// The surviving mutants in query.rs (lines 83-84, 216-217) are:
//   `delete !` — inverts the empty-check, so non-empty fields would produce no hint
//   `replace && with ||` — Stream strategy would produce a hint anyway
//   `replace == with !=` — Project strategy treated as non-Project, inverts the hint decision
//
// | Mutant        | Location   | Killed by |
// |---------------|------------|-----------|
// | delete !      | line 83,216| E2/F2 (Project + non-empty → must be Some) |
// | replace && || | line 84,217| E1/F1 (Stream + non-empty → must be None)  |
// | replace == != | line 84,217| E1+E2/F1+F2 together                        |

/// Adapter that records whether a projection hint was passed.
struct RecordingAdapter {
    results:              Vec<JsonbValue>,
    got_projection_hint:  Arc<AtomicBool>,
}

#[async_trait]
impl DatabaseAdapter for RecordingAdapter {
    async fn execute_with_projection(
        &self,
        _view: &str,
        projection: Option<&crate::schema::SqlProjectionHint>,
        _where_clause: Option<&WhereClause>,
        _limit: Option<u32>,
    ) -> Result<Vec<JsonbValue>> {
        if projection.is_some() {
            self.got_projection_hint.store(true, Ordering::SeqCst);
        }
        Ok(self.results.clone())
    }

    async fn execute_where_query(
        &self,
        _view: &str,
        _where_clause: Option<&WhereClause>,
        _limit: Option<u32>,
        _offset: Option<u32>,
    ) -> Result<Vec<JsonbValue>> {
        Ok(self.results.clone())
    }

    async fn health_check(&self) -> Result<()> {
        Ok(())
    }

    fn database_type(&self) -> crate::db::types::DatabaseType {
        crate::db::types::DatabaseType::PostgreSQL
    }

    fn pool_metrics(&self) -> crate::db::types::PoolMetrics {
        crate::db::types::PoolMetrics {
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

    async fn execute_function_call(
        &self,
        _function_name: &str,
        _args: &[serde_json::Value],
    ) -> Result<Vec<std::collections::HashMap<String, serde_json::Value>>> {
        Ok(vec![])
    }
}

impl MutationCapable for RecordingAdapter {}

/// E1: Requesting ≥80% of estimated fields triggers Stream strategy → no projection hint.
///
/// The planner estimates 10 total fields by default. Requesting 10 fields yields
/// 100% coverage → Stream strategy → no hint sent.
///
/// Kills `replace && with ||` at query.rs:217:
///   With `||`, `!empty || strategy == Project` = `true || false` → hint sent even for Stream.
/// Also kills `replace == with !=` at query.rs:217:
///   With `!=`, `!empty && strategy != Project` = `true && true` → hint sent for Stream.
#[tokio::test]
async fn execute_regular_query_stream_strategy_sends_no_projection_hint() {
    let schema = test_schema();
    let hint_flag = Arc::new(AtomicBool::new(false));
    let adapter = Arc::new(RecordingAdapter {
        results:             vec![JsonbValue::new(serde_json::json!({"id": "1"}))],
        got_projection_hint: hint_flag.clone(),
    });
    let executor = Executor::new(schema, adapter);

    // 10 fields → planner: 10/max(10,10) = 100% >= 80% threshold → Stream strategy.
    executor
        .execute("{ users { id name age email phone addr city country role status } }", None)
        .await
        .unwrap();
    assert!(
        !hint_flag.load(Ordering::SeqCst),
        "E1: Stream strategy (many fields) must NOT produce a projection hint"
    );
}

/// E2: Requesting few fields triggers Project strategy → projection hint IS sent.
///
/// Kills `delete !` at query.rs:216:
///   With `delete !`, `plan.projection_fields.is_empty()` on a non-empty list → false → no hint.
#[tokio::test]
async fn execute_regular_query_project_strategy_sends_projection_hint() {
    let schema = test_schema();
    let hint_flag = Arc::new(AtomicBool::new(false));
    let adapter = Arc::new(RecordingAdapter {
        results:             vec![JsonbValue::new(serde_json::json!({"id": "1", "name": "Alice"}))],
        got_projection_hint: hint_flag.clone(),
    });
    // 2 fields → 2/max(2,10) = 20% < 80% threshold → Project strategy → hint must be sent.
    let executor = Executor::new(schema, adapter);

    executor.execute("{ users { id name } }", None).await.unwrap();
    assert!(
        hint_flag.load(Ordering::SeqCst),
        "E2: Project strategy with non-empty fields must produce a projection hint"
    );
}

/// F1: Security path with many fields → Stream → no projection hint.
///
/// Kills `replace && with ||` at query.rs:84 and `replace == with !=` at query.rs:84.
#[tokio::test]
async fn execute_with_security_stream_strategy_sends_no_projection_hint() {
    let schema = test_schema();
    let hint_flag = Arc::new(AtomicBool::new(false));
    let adapter = Arc::new(RecordingAdapter {
        results:             vec![JsonbValue::new(serde_json::json!({"id": "1"}))],
        got_projection_hint: hint_flag.clone(),
    });
    let executor = Executor::new(schema, adapter);
    let ctx = make_security_ctx_with_roles("user-1", vec![]);

    // 10 fields → Stream strategy (same threshold as E1).
    executor
        .execute_with_security(
            "query { users { id name age email phone addr city country role status } }",
            None,
            &ctx,
        )
        .await
        .unwrap();
    assert!(
        !hint_flag.load(Ordering::SeqCst),
        "F1: Stream strategy must NOT produce a projection hint (security path)"
    );
}

/// F2: Security path with few fields → Project → projection hint IS sent.
///
/// Kills `delete !` at query.rs:83.
#[tokio::test]
async fn execute_with_security_project_strategy_sends_projection_hint() {
    let schema = test_schema();
    let hint_flag = Arc::new(AtomicBool::new(false));
    let adapter = Arc::new(RecordingAdapter {
        results:             vec![JsonbValue::new(serde_json::json!({"id": "1", "name": "Alice"}))],
        got_projection_hint: hint_flag.clone(),
    });
    let executor = Executor::new(schema, adapter);
    let ctx = make_security_ctx_with_roles("user-1", vec![]);

    executor
        .execute_with_security("query { users { id name } }", None, &ctx)
        .await
        .unwrap();
    assert!(
        hint_flag.load(Ordering::SeqCst),
        "F2: Project strategy with non-empty fields must produce a projection hint (security path)"
    );
}

// ── G: Relay pagination — mutation-targeted tests ─────────────────────────────
//
// Surviving mutants in execute_relay_query (query.rs):
//
// | Mutant              | Line | What changes              | Killed by |
// |---------------------|------|---------------------------|-----------|
// | replace && with ||  | 378  | direction logic           | G1c       |
// | replace + with -    | 385  | fetch_limit undercount    | G2        |
// | replace + with *    | 385  | fetch_limit = page_size*1 | G2        |
// | replace == with !=  | 412  | totalCount detection      | G3        |
// | replace > with ==   | 437  | has_extra detection       | G4        |
// | replace > with <    | 437  | has_extra detection       | G4b       |
// | replace > with >=   | 437  | has_extra detection       | G4c       |
// | replace == with !=  | 481  | startCursor assignment    | G5        |

/// Relay mock adapter that records the `forward` flag and `limit` passed,
/// and returns a configurable slice of rows.
struct RecordingRelayAdapter {
    rows:                     Vec<JsonbValue>,
    last_forward:             Arc<AtomicBool>,
    last_limit:               Arc<AtomicU32>,
    last_include_total_count: Arc<AtomicBool>,
}

impl RecordingRelayAdapter {
    fn new(rows: Vec<JsonbValue>) -> (Arc<Self>, Arc<AtomicBool>, Arc<AtomicU32>, Arc<AtomicBool>) {
        let forward_flag = Arc::new(AtomicBool::new(true));
        let limit_val = Arc::new(AtomicU32::new(0));
        let total_flag = Arc::new(AtomicBool::new(false));
        let adapter = Arc::new(Self {
            rows,
            last_forward:             forward_flag.clone(),
            last_limit:               limit_val.clone(),
            last_include_total_count: total_flag.clone(),
        });
        (adapter, forward_flag, limit_val, total_flag)
    }
}

#[async_trait]
impl DatabaseAdapter for RecordingRelayAdapter {
    async fn execute_with_projection(
        &self,
        _view: &str,
        _projection: Option<&crate::schema::SqlProjectionHint>,
        _where_clause: Option<&WhereClause>,
        _limit: Option<u32>,
    ) -> Result<Vec<JsonbValue>> {
        Ok(self.rows.clone())
    }

    async fn execute_where_query(
        &self,
        _view: &str,
        _where_clause: Option<&WhereClause>,
        _limit: Option<u32>,
        _offset: Option<u32>,
    ) -> Result<Vec<JsonbValue>> {
        Ok(self.rows.clone())
    }

    async fn health_check(&self) -> Result<()> {
        Ok(())
    }

    fn database_type(&self) -> crate::db::types::DatabaseType {
        crate::db::types::DatabaseType::PostgreSQL
    }

    fn pool_metrics(&self) -> crate::db::types::PoolMetrics {
        crate::db::types::PoolMetrics {
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

    async fn execute_function_call(
        &self,
        _function_name: &str,
        _args: &[serde_json::Value],
    ) -> Result<Vec<std::collections::HashMap<String, serde_json::Value>>> {
        Ok(vec![])
    }
}

impl MutationCapable for RecordingRelayAdapter {}

#[async_trait]
impl RelayDatabaseAdapter for RecordingRelayAdapter {
    async fn execute_relay_page(
        &self,
        _view: &str,
        _cursor_column: &str,
        _after: Option<CursorValue>,
        _before: Option<CursorValue>,
        limit: u32,
        forward: bool,
        _where_clause: Option<&crate::db::WhereClause>,
        _order_by: Option<&[OrderByClause]>,
        include_total_count: bool,
    ) -> Result<RelayPageResult> {
        self.last_forward.store(forward, Ordering::SeqCst);
        self.last_limit.store(limit, Ordering::SeqCst);
        self.last_include_total_count.store(include_total_count, Ordering::SeqCst);
        let total_count = if include_total_count {
            Some(self.rows.len() as u64)
        } else {
            None
        };
        let rows = self.rows.iter().take(limit as usize).cloned().collect();
        Ok(RelayPageResult { rows, total_count })
    }
}

/// Build a schema with a relay-enabled `users` query (Int64 cursor on pk_user).
fn relay_query_schema() -> crate::schema::CompiledSchema {
    let mut schema = crate::schema::CompiledSchema::new();
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
        relay:               true,
        relay_cursor_column: Some("pk_user".to_string()),
        relay_cursor_type:   CursorType::Int64,
        inject_params:       Default::default(),
        cache_ttl_seconds:   None,
        additional_views:    vec![],
        requires_role:       None,
        rest:                None,
    });
    schema
}

/// Build 3 relay rows with sequential pk_user values.
fn relay_rows() -> Vec<JsonbValue> {
    vec![
        JsonbValue::new(serde_json::json!({"id": "1", "name": "Alice", "pk_user": 1})),
        JsonbValue::new(serde_json::json!({"id": "2", "name": "Bob",   "pk_user": 2})),
        JsonbValue::new(serde_json::json!({"id": "3", "name": "Carol", "pk_user": 3})),
    ]
}

/// G1: `first` given and no `last` → forward direction.
#[tokio::test]
async fn relay_forward_direction_when_first_given() {
    let (adapter, forward_flag, _, _) = RecordingRelayAdapter::new(relay_rows());
    let executor = Executor::new_with_relay(relay_query_schema(), adapter);

    executor
        .execute("{ users { edges { node { id } } pageInfo { hasNextPage } } }", None)
        .await
        .unwrap();
    assert!(
        forward_flag.load(Ordering::SeqCst),
        "G1: no 'last' arg → forward must be true"
    );
}

/// G1b: `last` given and no `first` → backward direction.
#[tokio::test]
async fn relay_backward_direction_when_only_last_given() {
    let (adapter, forward_flag, _, _) = RecordingRelayAdapter::new(relay_rows());
    let executor = Executor::new_with_relay(relay_query_schema(), adapter);

    let vars = serde_json::json!({"last": 2});
    executor
        .execute(
            "query($last: Int) { users(last: $last) { edges { node { id } } pageInfo { hasPreviousPage } } }",
            Some(&vars),
        )
        .await
        .unwrap();
    assert!(
        !forward_flag.load(Ordering::SeqCst),
        "G1b: only 'last' given → forward must be false"
    );
}

/// G1c: When BOTH `first` AND `last` are given, FORWARD wins.
///
/// Kills `replace && with ||` at query.rs:378:
///   Original: `last.is_some() && first.is_none()` — backward only if last given AND first absent.
///   Mutated:  `last.is_some() || first.is_none()` — backward if last given OR first absent.
///   With both first+last given: original → forward; mutated → backward.
#[tokio::test]
async fn relay_forward_wins_when_both_first_and_last_given() {
    let (adapter, forward_flag, _, _) = RecordingRelayAdapter::new(relay_rows());
    let executor = Executor::new_with_relay(relay_query_schema(), adapter);

    let vars = serde_json::json!({"first": 2, "last": 3});
    executor
        .execute(
            "query($first: Int, $last: Int) { users(first: $first, last: $last) { edges { node { id } } pageInfo { hasNextPage } } }",
            Some(&vars),
        )
        .await
        .unwrap();
    assert!(
        forward_flag.load(Ordering::SeqCst),
        "G1c: when both first+last given, forward must win (first takes priority)"
    );
}

/// G2: hasNextPage is true when the adapter returns page_size + 1 rows.
///
/// Kills both `replace + with -` and `replace + with *` at query.rs:385:
///   With `- 1`: fetch_limit = page_size - 1 → adapter only asked for 1 row → no extra detected.
///   With `* 1`: fetch_limit = page_size (same as page_size) → no over-fetch → hasNextPage false.
#[tokio::test]
async fn relay_has_next_page_when_adapter_returns_extra_row() {
    // 3 rows available; request first=2 → fetch_limit should be 3 → adapter returns 3 rows.
    let (adapter, _, limit_val, _) = RecordingRelayAdapter::new(relay_rows());
    let executor = Executor::new_with_relay(relay_query_schema(), adapter);

    let vars = serde_json::json!({"first": 2});
    let json = executor
        .execute_json(
            "query($first: Int) { users(first: $first) { edges { node { id } } pageInfo { hasNextPage } } }",
            Some(&vars),
        )
        .await
        .unwrap();

    // Verify the executor requested page_size + 1 = 3 rows from the adapter.
    assert_eq!(
        limit_val.load(Ordering::SeqCst),
        3,
        "G2: fetch_limit must be page_size + 1 = 3"
    );

    // Verify hasNextPage reflects the extra row.
    let has_next = json["data"]["users"]["pageInfo"]["hasNextPage"].as_bool().unwrap_or(false);
    assert!(has_next, "G2: hasNextPage must be true when adapter returns page_size + 1 rows");
}

/// G3: totalCount is included in the response when the query selects it.
///
/// Kills `replace == with !=` at query.rs:412:
///   With mutant, `sel.name == query_def.name` becomes `!=` → the find() looks in
///   the WRONG field for totalCount → include_total_count stays false → totalCount absent.
#[tokio::test]
async fn relay_total_count_included_when_selected() {
    let (adapter, _, _, total_flag) = RecordingRelayAdapter::new(relay_rows());
    let executor = Executor::new_with_relay(relay_query_schema(), adapter);

    let json = executor
        .execute_json(
            "{ users { edges { node { id } } totalCount } }",
            None,
        )
        .await
        .unwrap();

    // Adapter must have been asked for totalCount.
    assert!(
        total_flag.load(Ordering::SeqCst),
        "G3: include_total_count must be true when query selects totalCount"
    );

    // totalCount must appear in the response.
    let tc = &json["data"]["users"]["totalCount"];
    assert!(
        !tc.is_null(),
        "G3: totalCount must be present in response, got: {json}"
    );
    assert_eq!(tc.as_u64().unwrap(), 3, "G3: totalCount must equal number of available rows");
}

/// G4: hasNextPage is false when the adapter returns exactly page_size rows.
///
/// Kills `replace > with >=` at query.rs:437:
///   With `>=`, len == page_size → has_extra = true → hasNextPage = true (wrong).
#[tokio::test]
async fn relay_no_next_page_when_adapter_returns_exactly_page_size_rows() {
    // Exactly 2 rows; request first=2 → fetch_limit=3 → adapter returns min(3,2)=2.
    let rows = vec![
        JsonbValue::new(serde_json::json!({"id": "1", "name": "Alice", "pk_user": 1})),
        JsonbValue::new(serde_json::json!({"id": "2", "name": "Bob",   "pk_user": 2})),
    ];
    let (adapter, _, _, _) = RecordingRelayAdapter::new(rows);
    let executor = Executor::new_with_relay(relay_query_schema(), adapter);

    let vars = serde_json::json!({"first": 2});
    let json = executor
        .execute_json(
            "query($first: Int) { users(first: $first) { edges { node { id } } pageInfo { hasNextPage } } }",
            Some(&vars),
        )
        .await
        .unwrap();

    let has_next = json["data"]["users"]["pageInfo"]["hasNextPage"].as_bool().unwrap_or(true);
    assert!(!has_next, "G4: hasNextPage must be false when result has exactly page_size rows");
}

/// G4b: hasNextPage is false when fewer than page_size rows are returned.
///
/// Kills `replace > with <` at query.rs:437:
///   With `<`, rows.len() < page_size → has_extra = true for partial pages (wrong).
#[tokio::test]
async fn relay_no_next_page_when_adapter_returns_fewer_than_page_size_rows() {
    // 1 row; request first=2 → fetch_limit=3 → adapter returns 1.
    let rows = vec![JsonbValue::new(serde_json::json!({"id": "1", "name": "Alice", "pk_user": 1}))];
    let (adapter, _, _, _) = RecordingRelayAdapter::new(rows);
    let executor = Executor::new_with_relay(relay_query_schema(), adapter);

    let vars = serde_json::json!({"first": 2});
    let json = executor
        .execute_json(
            "query($first: Int) { users(first: $first) { edges { node { id } } pageInfo { hasNextPage } } }",
            Some(&vars),
        )
        .await
        .unwrap();

    let has_next = json["data"]["users"]["pageInfo"]["hasNextPage"].as_bool().unwrap_or(true);
    assert!(!has_next, "G4b: hasNextPage must be false when result has fewer than page_size rows");
}

/// G5: pageInfo.startCursor must correspond to the FIRST edge's cursor.
///
/// Kills `replace == with !=` at query.rs:481:
///   With mutant, `if i == 0` becomes `if i != 0` → startCursor is set for every row
///   EXCEPT the first, leaving it as the last row's cursor at the end.
#[tokio::test]
async fn relay_start_cursor_is_first_edge_cursor() {
    // 3 rows; request first=3 → edges = [Alice(1), Bob(2), Carol(3)].
    let (adapter, _, _, _) = RecordingRelayAdapter::new(relay_rows());
    let executor = Executor::new_with_relay(relay_query_schema(), adapter);

    let vars = serde_json::json!({"first": 3});
    let json = executor
        .execute_json(
            "query($first: Int) { users(first: $first) { edges { cursor node { id pk_user } } pageInfo { startCursor endCursor } } }",
            Some(&vars),
        )
        .await
        .unwrap();

    let edges = json["data"]["users"]["edges"].as_array().expect("edges must be an array");
    assert_eq!(edges.len(), 3, "G5: must have 3 edges");

    let first_edge_cursor = edges[0]["cursor"].as_str().expect("first edge must have cursor");
    let start_cursor = json["data"]["users"]["pageInfo"]["startCursor"]
        .as_str()
        .expect("startCursor must be a string");

    assert_eq!(
        start_cursor, first_edge_cursor,
        "G5: pageInfo.startCursor must equal the first edge's cursor"
    );

    // Also verify endCursor != startCursor (so we know they're distinct).
    let end_cursor = json["data"]["users"]["pageInfo"]["endCursor"]
        .as_str()
        .expect("endCursor must be a string");
    assert_ne!(
        start_cursor, end_cursor,
        "G5: startCursor and endCursor must differ for a multi-row page"
    );
}
