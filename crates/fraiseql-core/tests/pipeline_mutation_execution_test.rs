#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

//! Pipeline 3 integration tests — mutation execution end-to-end.
//!
//! Drives the **mutation execution pipeline** end-to-end:
//!
//!   golden fixture JSON  →  `CompiledSchema::from_json()`  →  `Executor::execute()`
//!   → `execute_mutation_query_with_security()`  →  `adapter.execute_function_call()`
//!
//! Tests that `sql_source` is correctly threaded from the compiled schema into the
//! adapter call — catching the class of regression described in issue #53 for mutations.

use std::{collections::HashMap, sync::Arc};

use async_trait::async_trait;
use chrono::Utc;
use fraiseql_core::{
    db::{
        traits::{DatabaseAdapter, SupportsMutations},
        types::{DatabaseType, JsonbValue, OrderByClause, PoolMetrics},
        where_clause::WhereClause,
    },
    error::Result,
    runtime::Executor,
    schema::{CompiledSchema, SqlProjectionHint},
    security::SecurityContext,
};
use serde_json::json;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn mutation_success_row() -> HashMap<String, serde_json::Value> {
    let mut row = HashMap::new();

    row.insert("succeeded".to_string(), json!(true));
    row.insert("state_changed".to_string(), json!(true));
    row.insert("message".to_string(), json!("ok"));
    row.insert(
        "entity".to_string(),
        json!({"id": "abc-123", "email": "a@b.com", "name": "Alice"}),
    );
    row.insert("entity_type".to_string(), json!("User"));
    row
}

fn order_success_row() -> HashMap<String, serde_json::Value> {
    let mut row = HashMap::new();

    row.insert("succeeded".to_string(), json!(true));
    row.insert("state_changed".to_string(), json!(true));
    row.insert("message".to_string(), json!("ok"));
    row.insert(
        "entity".to_string(),
        json!({"id": "order-456", "tenant_id": "t-1", "amount": "99.99", "status": "pending"}),
    );
    row.insert("entity_type".to_string(), json!("Order"));
    row
}

fn admin_security_context() -> SecurityContext {
    SecurityContext {
        user_id:          "user-123".to_string(),
        tenant_id:        Some("tenant-456".to_string()),
        roles:            vec!["admin".to_string()],
        scopes:           vec![],
        attributes:       HashMap::new(),
        request_id:       "req-test".to_string(),
        ip_address:       None,
        authenticated_at: Utc::now(),
        expires_at:       Utc::now() + chrono::Duration::hours(1),
        issuer:           None,
        audience:         None,
    }
}

// ---------------------------------------------------------------------------
// Mock adapter that records which function name was called
// ---------------------------------------------------------------------------

struct RecordingMockAdapter {
    called_fn:    std::sync::Mutex<Option<String>>,
    called_args:  std::sync::Mutex<Vec<serde_json::Value>>,
    response_row: HashMap<String, serde_json::Value>,
}

impl RecordingMockAdapter {
    const fn new(response_row: HashMap<String, serde_json::Value>) -> Self {
        Self {
            called_fn: std::sync::Mutex::new(None),
            called_args: std::sync::Mutex::new(vec![]),
            response_row,
        }
    }

    fn called_function(&self) -> Option<String> {
        self.called_fn.lock().unwrap().clone()
    }

    fn called_args(&self) -> Vec<serde_json::Value> {
        self.called_args.lock().unwrap().clone()
    }
}

// Reason: DatabaseAdapter is defined with #[async_trait]; all implementations must match
// its transformed method signatures to satisfy the trait contract
#[async_trait]
impl DatabaseAdapter for RecordingMockAdapter {
    async fn execute_with_projection(
        &self,
        _view: &str,
        _projection: Option<&SqlProjectionHint>,
        _where_clause: Option<&WhereClause>,
        _limit: Option<u32>,
        _offset: Option<u32>,
        _order_by: Option<&[OrderByClause]>,
    ) -> Result<Vec<JsonbValue>> {
        Ok(vec![])
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
    ) -> Result<Vec<HashMap<String, serde_json::Value>>> {
        Ok(vec![])
    }

    async fn execute_parameterized_aggregate(
        &self,
        _sql: &str,
        _params: &[serde_json::Value],
    ) -> Result<Vec<HashMap<String, serde_json::Value>>> {
        Ok(vec![])
    }

    async fn execute_function_call(
        &self,
        function_name: &str,
        args: &[serde_json::Value],
    ) -> Result<Vec<HashMap<String, serde_json::Value>>> {
        *self.called_fn.lock().unwrap() = Some(function_name.to_string());
        *self.called_args.lock().unwrap() = args.to_vec();
        Ok(vec![self.response_row.clone()])
    }
}

impl SupportsMutations for RecordingMockAdapter {}

// ---------------------------------------------------------------------------
// Mutation executor uses sql_source from compiled schema
// ---------------------------------------------------------------------------

/// Pipeline 3: full mutation execution path drives `sql_source` from the compiled schema.
///
/// Verifies that `Executor::execute()` dispatches to `execute_function_call`
/// using exactly the `sql_source` stored in the `MutationDefinition`, not any
/// hand-built or default value — catching regressions where `sql_source` is lost
/// during schema loading (issue #53 class of bug, mutation path).
#[tokio::test]
async fn mutation_executor_uses_sql_source_from_compiled_schema() {
    let json = include_str!("../../../tests/fixtures/golden/01-basic-query-mutation.json");
    let schema = CompiledSchema::from_json(json).expect("golden fixture must parse");

    // Assert the golden fixture has the expected sql_source before testing
    let m = schema
        .find_mutation("createUser")
        .expect("'createUser' must be in golden fixture 01");
    assert_eq!(
        m.sql_source.as_deref(),
        Some("fn_create_user"),
        "pre-condition: golden fixture 01 must have sql_source=fn_create_user"
    );

    let mock = Arc::new(RecordingMockAdapter::new(mutation_success_row()));
    let executor = Executor::new(schema, Arc::clone(&mock));
    let vars = serde_json::json!({"email": "a@b.com", "name": "Alice"});

    let result = executor.execute(r"mutation { createUser { id } }", Some(&vars)).await;

    assert!(result.is_ok(), "mutation must succeed: {result:?}");

    // The adapter must have been called with the sql_source from the schema
    assert_eq!(
        mock.called_function().as_deref(),
        Some("fn_create_user"),
        "executor must pass sql_source to execute_function_call — regression for #53"
    );
}

/// Pipeline 3: mutation arguments are forwarded to `execute_function_call`.
///
/// Confirms that the arguments extracted from the GraphQL operation are
/// passed through to `execute_function_call` in the correct order.
#[tokio::test]
async fn mutation_executor_passes_arguments_to_function_call() {
    let json = include_str!("../../../tests/fixtures/golden/01-basic-query-mutation.json");
    let schema = CompiledSchema::from_json(json).expect("golden fixture must parse");

    let mock = Arc::new(RecordingMockAdapter::new(mutation_success_row()));
    let executor = Executor::new(schema, Arc::clone(&mock));
    let vars = serde_json::json!({"email": "test@example.com", "name": "Bob"});

    let result = executor.execute(r"mutation { createUser { id } }", Some(&vars)).await;

    assert!(result.is_ok(), "mutation with arguments must succeed: {result:?}");

    // Some arguments must have been forwarded
    let args = mock.called_args();
    assert!(
        !args.is_empty(),
        "mutation arguments must be forwarded to execute_function_call"
    );
}

/// Pipeline 3: mutation response is wrapped in a GraphQL data envelope.
///
/// Confirms that the JSON returned by `execute()` is a valid GraphQL response
/// with a `data` key (not `errors`) when the adapter returns a success row.
#[tokio::test]
async fn mutation_executor_wraps_response_in_data_envelope() {
    let json = include_str!("../../../tests/fixtures/golden/01-basic-query-mutation.json");
    let schema = CompiledSchema::from_json(json).expect("golden fixture must parse");

    let mock = Arc::new(RecordingMockAdapter::new(mutation_success_row()));
    let executor = Executor::new(schema, Arc::clone(&mock));
    let vars = serde_json::json!({"email": "a@b.com", "name": "Alice"});

    let result = executor
        .execute(r"mutation { createUser { id } }", Some(&vars))
        .await
        .expect("mutation must succeed");

    assert!(result.get("data").is_some(), "response must have 'data' key");
    assert!(
        result.get("errors").is_none(),
        "successful mutation must not produce 'errors': {result}"
    );
}

// ---------------------------------------------------------------------------
// inject_params are appended from JWT security context
// ---------------------------------------------------------------------------

/// Pipeline 3: `inject_params` from the compiled schema are resolved from the
/// `SecurityContext` and appended to `execute_function_call` arguments.
///
/// Uses golden fixture 05 which has `inject_params: {user_id: jwt:sub, tenant_id: jwt:org_id}`
/// on the `createOrder` mutation.
#[tokio::test]
async fn mutation_executor_appends_inject_params_from_jwt() {
    let json = include_str!("../../../tests/fixtures/golden/05-security-inject-cache.json");
    let schema = CompiledSchema::from_json(json).expect("golden fixture must parse");

    // Verify the fixture has inject_params before running
    let m = schema
        .find_mutation("createOrder")
        .expect("'createOrder' must be in golden fixture 05");
    assert!(
        !m.inject_params.is_empty(),
        "pre-condition: createOrder must have inject_params in fixture 05"
    );

    let mock = Arc::new(RecordingMockAdapter::new(order_success_row()));
    let executor = Executor::new(schema, Arc::clone(&mock));

    let ctx = admin_security_context();
    let vars = serde_json::json!({"amount": "99.99"});

    let result = executor
        .execute_with_security(r"mutation { createOrder { id } }", Some(&vars), &ctx)
        .await;

    assert!(result.is_ok(), "mutation with inject_params must succeed: {result:?}");

    // The adapter must have been called with the sql_source from the schema
    assert_eq!(
        mock.called_function().as_deref(),
        Some("fn_create_order"),
        "executor must use sql_source fn_create_order from fixture 05"
    );

    // Inject params must be appended — args must include the resolved values
    let args = mock.called_args();
    assert!(!args.is_empty(), "inject_params must cause at least one argument to be passed");
    // The injected user_id (from JWT "sub") must appear in the args
    let has_user_id = args.iter().any(|v| v.as_str() == Some("user-123"));
    assert!(
        has_user_id,
        "resolved inject_param user_id='user-123' must appear in args: {args:?}"
    );
}

// ---------------------------------------------------------------------------
// Error path: selection filtering and array handling (#214)
// ---------------------------------------------------------------------------

fn mutation_error_row() -> HashMap<String, serde_json::Value> {
    let mut row = HashMap::new();

    row.insert("succeeded".to_string(), json!(false));
    row.insert("state_changed".to_string(), json!(false));
    row.insert("error_class".to_string(), json!("conflict"));
    row.insert("message".to_string(), json!("email already exists"));
    row.insert(
        "error_detail".to_string(),
        json!({
            "message": "email already exists",
            "conflicting_id": "existing-user-id",
            "code": 409,
            "affected_ids": ["id-1", "id-2"],
            "details": {"field": "email", "rule": "unique"}
        }),
    );
    row
}

/// Error path: selection filtering restricts error fields to those requested.
///
/// When the client requests only `{ message code }` on an error union member,
/// unrequested fields like `conflicting_id` and `affected_ids` must not appear.
#[tokio::test]
async fn error_path_applies_selection_filtering() {
    let json = include_str!("../../../tests/fixtures/golden/09-mutation-error-union.json");
    let schema = CompiledSchema::from_json(json).expect("fixture must parse");

    let mock = Arc::new(RecordingMockAdapter::new(mutation_error_row()));
    let executor = Executor::new(schema, Arc::clone(&mock));
    let vars = json!({"email": "dup@example.com", "name": "Alice"});

    // Only request message and code (not conflicting_id, affected_ids, details)
    let result = executor
        .execute(
            r#"mutation { createUser(email: "dup@example.com", name: "Alice") { ... on DuplicateEmailError { message code } } }"#,
            Some(&vars),
        )
        .await
        .expect("mutation must succeed even on error outcome");

    let data = &result["data"]["createUser"];

    assert_eq!(data["__typename"], "DuplicateEmailError");
    assert_eq!(data["message"], "email already exists");
    assert_eq!(data["code"], 409);

    // Fields NOT in the selection set must be absent
    assert!(
        data.get("conflicting_id").is_none(),
        "unrequested field 'conflicting_id' must be filtered out: {data}"
    );
    assert!(
        data.get("affected_ids").is_none(),
        "unrequested field 'affected_ids' must be filtered out: {data}"
    );
    assert!(
        data.get("details").is_none(),
        "unrequested field 'details' must be filtered out: {data}"
    );
}

/// Error path: array fields are correctly populated from `error_detail`.
#[tokio::test]
async fn error_path_populates_array_fields() {
    let json = include_str!("../../../tests/fixtures/golden/09-mutation-error-union.json");
    let schema = CompiledSchema::from_json(json).expect("fixture must parse");

    let mock = Arc::new(RecordingMockAdapter::new(mutation_error_row()));
    let executor = Executor::new(schema, Arc::clone(&mock));
    let vars = json!({"email": "dup@example.com", "name": "Alice"});

    // Request the array field
    let result = executor
        .execute(
            r#"mutation { createUser(email: "dup@example.com", name: "Alice") { ... on DuplicateEmailError { message affected_ids } } }"#,
            Some(&vars),
        )
        .await
        .expect("mutation must succeed");

    let data = &result["data"]["createUser"];

    assert_eq!(data["__typename"], "DuplicateEmailError");
    let arr = data["affected_ids"]
        .as_array()
        .expect("affected_ids must be an array");
    assert_eq!(arr.len(), 2);
    assert_eq!(arr[0], "id-1");
    assert_eq!(arr[1], "id-2");
}

/// Error path: nested object field is correctly populated from `error_detail`.
#[tokio::test]
async fn error_path_populates_nested_object_fields() {
    let json = include_str!("../../../tests/fixtures/golden/09-mutation-error-union.json");
    let schema = CompiledSchema::from_json(json).expect("fixture must parse");

    let mock = Arc::new(RecordingMockAdapter::new(mutation_error_row()));
    let executor = Executor::new(schema, Arc::clone(&mock));
    let vars = json!({"email": "dup@example.com", "name": "Alice"});

    let result = executor
        .execute(
            r#"mutation { createUser(email: "dup@example.com", name: "Alice") { ... on DuplicateEmailError { message details } } }"#,
            Some(&vars),
        )
        .await
        .expect("mutation must succeed");

    let data = &result["data"]["createUser"];

    assert_eq!(data["details"]["field"], "email");
    assert_eq!(data["details"]["rule"], "unique");
}

/// Cascade JSONB from `mutation_response_v2.cascade` is surfaced in the response.
///
/// When the DB function returns a non-null `cascade` JSONB, the executor must
/// inject it as a `"cascade"` key on the projected entity object so that clients
/// receive the full graphql-cascade wire format
/// (`{ updated[], deleted[], invalidations[], metadata }`).
#[tokio::test]
async fn mutation_cascade_json_is_surfaced_in_response() {
    let json = include_str!("../../../tests/fixtures/golden/01-basic-query-mutation.json");
    let schema = CompiledSchema::from_json(json).expect("golden fixture must parse");

    let cascade_payload = json!({
        "updated": [
            { "__typename": "User", "id": "abc-123" }
        ],
        "deleted": [],
        "invalidations": [],
        "metadata": { "triggered_by": "createUser" }
    });

    let mut row = mutation_success_row();
    row.insert("cascade".to_string(), cascade_payload.clone());

    let mock = Arc::new(RecordingMockAdapter::new(row));
    let executor = Executor::new(schema, Arc::clone(&mock));
    let vars = serde_json::json!({"email": "a@b.com", "name": "Alice"});

    let result = executor
        .execute(r"mutation { createUser { id } }", Some(&vars))
        .await
        .expect("mutation must succeed");

    let entity = &result["data"]["createUser"];
    assert!(entity.get("cascade").is_some(), "cascade must be present in response: {result}");
    assert_eq!(entity["cascade"]["updated"][0]["__typename"], "User");
    assert_eq!(entity["cascade"]["metadata"]["triggered_by"], "createUser");
}

/// Pipeline 3: mutation with `inject_params` fails when no security context provided.
///
/// A mutation that requires `inject_params` (resolved from JWT claims) cannot
/// execute without a `SecurityContext`. The executor must return a Validation
/// error rather than silently ignoring the inject configuration.
#[tokio::test]
async fn mutation_executor_rejects_inject_params_without_security_context() {
    let json = include_str!("../../../tests/fixtures/golden/05-security-inject-cache.json");
    let schema = CompiledSchema::from_json(json).expect("golden fixture must parse");

    let mock = Arc::new(RecordingMockAdapter::new(order_success_row()));
    let executor = Executor::new(schema, Arc::clone(&mock));

    // Call execute() (unauthenticated path) — must fail with Validation error
    let vars = serde_json::json!({"amount": "99.99"});
    let result = executor.execute(r"mutation { createOrder { id } }", Some(&vars)).await;

    assert!(result.is_err(), "mutation with inject_params must fail without SecurityContext");
}
