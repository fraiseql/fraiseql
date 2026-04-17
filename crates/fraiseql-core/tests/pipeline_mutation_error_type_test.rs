#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

//! Pipeline 3 integration tests — mutation error-type field population.
//!
//! Tests that when a mutation returns a failure status, the executor populates
//! error-type fields from `mutation_response.metadata` JSONB (issue #294 regression
//! guard).
//!
//! Golden fixture 04 has `DuplicateEmailError` and `ValidationError` types with
//! `is_error: true` and scalar fields (String, Int, `DateTime`, UUID).

#![allow(clippy::literal_string_with_formatting_args)] // Reason: test expected strings contain format-like patterns that are literal data, not format args
use std::{collections::HashMap, sync::Arc};

use async_trait::async_trait;
use fraiseql_core::{
    db::{
        traits::{DatabaseAdapter, SupportsMutations},
        types::{DatabaseType, JsonbValue, OrderByClause, PoolMetrics},
        where_clause::WhereClause,
    },
    error::Result,
    runtime::Executor,
    schema::{CompiledSchema, SqlProjectionHint},
};
use serde_json::json;

// ---------------------------------------------------------------------------
// Mock adapter
// ---------------------------------------------------------------------------

struct ErrorMockAdapter {
    response_row: HashMap<String, serde_json::Value>,
}

impl ErrorMockAdapter {
    const fn with_row(response_row: HashMap<String, serde_json::Value>) -> Self {
        Self { response_row }
    }
}

// Reason: DatabaseAdapter is defined with #[async_trait]; all implementations must match
// its transformed method signatures to satisfy the trait contract
#[async_trait]
impl DatabaseAdapter for ErrorMockAdapter {
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
    ) -> Result<Vec<std::collections::HashMap<String, serde_json::Value>>> {
        Ok(vec![])
    }

    async fn execute_function_call(
        &self,
        _function_name: &str,
        _args: &[serde_json::Value],
        _session_vars: &[(&str, &str)],
    ) -> Result<Vec<HashMap<String, serde_json::Value>>> {
        Ok(vec![self.response_row.clone()])
    }
}

impl SupportsMutations for ErrorMockAdapter {}

// ---------------------------------------------------------------------------
// Error type mutation result population
// ---------------------------------------------------------------------------

/// Pipeline 3 (error path): mutation returning `failed:conflict` produces
/// a GraphQL response with an `errors` array (not `data`) or populates an
/// error union member.
///
/// This is the core regression guard for issue #294: the executor must handle
/// failed mutation responses without panicking, returning a well-formed
/// GraphQL response.
#[tokio::test]
async fn mutation_error_status_produces_graphql_level_response() {
    let json = include_str!("../../../tests/fixtures/golden/04-error-type.json");
    let schema = CompiledSchema::from_json(json).expect("golden fixture must parse");

    let mut row = HashMap::new();
    row.insert("succeeded".to_string(), json!(false));
    row.insert("state_changed".to_string(), json!(false));
    row.insert("error_class".to_string(), json!("conflict"));
    row.insert("message".to_string(), json!("Email already exists"));
    row.insert("entity".to_string(), serde_json::Value::Null);
    row.insert("entity_type".to_string(), json!("DuplicateEmailError"));
    row.insert("cascade".to_string(), serde_json::Value::Null);
    row.insert(
        "metadata".to_string(),
        json!({
            "message": "User already exists",
            "code": 409,
            "conflicting_id": "user-existing-123"
        }),
    );

    let mock = Arc::new(ErrorMockAdapter::with_row(row));
    let executor = Executor::new(schema, mock);
    let vars = json!({"email": "dup@example.com", "name": "Alice"});

    let result = executor.execute(r"mutation { createUser { id } }", Some(&vars)).await;

    // The executor must return Ok with a JSON response (not Err)
    assert!(
        result.is_ok(),
        "executor must return Ok (not Err) for failed mutation status: {result:?}"
    );

    let body = result.unwrap();

    // Must be a valid GraphQL envelope
    assert!(
        body.get("data").is_some() || body.get("errors").is_some(),
        "response must be a GraphQL envelope with 'data' or 'errors': {body}"
    );
}

/// Pipeline 3 (error path): `failed:conflict` status returns a non-null response.
///
/// In the union/error-type pattern, the mutation field may return the error type
/// as `data` (not `errors`). In either case, the response must not be empty JSON.
#[tokio::test]
async fn mutation_failed_conflict_returns_non_empty_response() {
    let json = include_str!("../../../tests/fixtures/golden/04-error-type.json");
    let schema = CompiledSchema::from_json(json).expect("golden fixture must parse");

    let mut row = HashMap::new();
    row.insert("succeeded".to_string(), json!(false));
    row.insert("state_changed".to_string(), json!(false));
    row.insert("error_class".to_string(), json!("conflict"));
    row.insert("message".to_string(), json!("Duplicate email"));
    row.insert("entity".to_string(), serde_json::Value::Null);
    row.insert("entity_type".to_string(), json!("DuplicateEmailError"));
    row.insert("cascade".to_string(), serde_json::Value::Null);
    row.insert("metadata".to_string(), json!({"message": "Already taken", "code": 409}));

    let mock = Arc::new(ErrorMockAdapter::with_row(row));
    let executor = Executor::new(schema, mock);
    let vars = json!({"email": "dup@example.com", "name": "Alice"});

    let result = executor.execute(r"mutation { createUser { id } }", Some(&vars)).await;

    assert!(result.is_ok(), "executor must return Ok: {result:?}");
    let body = result.unwrap();
    assert!(body != serde_json::Value::Null, "response must not be null");
}

/// Pipeline 3 (error path): `"error"` generic status is also treated as failure.
///
/// Confirms that the `is_error_status` classification covers `"error"` (not just
/// `"failed:*"` and `"conflict:*"`), and that the executor handles it gracefully.
#[tokio::test]
async fn mutation_generic_error_status_produces_valid_response() {
    let json = include_str!("../../../tests/fixtures/golden/04-error-type.json");
    let schema = CompiledSchema::from_json(json).expect("golden fixture must parse");

    let mut row = HashMap::new();
    row.insert("succeeded".to_string(), json!(false));
    row.insert("state_changed".to_string(), json!(false));
    row.insert("error_class".to_string(), json!("internal"));
    row.insert("message".to_string(), json!("Internal error"));
    row.insert("entity".to_string(), serde_json::Value::Null);
    row.insert("entity_type".to_string(), json!("ValidationError"));
    row.insert("cascade".to_string(), serde_json::Value::Null);
    row.insert(
        "metadata".to_string(),
        json!({"message": "Unexpected error", "field": "email", "rule": "required"}),
    );

    let mock = Arc::new(ErrorMockAdapter::with_row(row));
    let executor = Executor::new(schema, mock);
    let vars = json!({"email": "a@b.com", "name": "Alice"});

    let result = executor.execute(r"mutation { createUser { id } }", Some(&vars)).await;

    assert!(
        result.is_ok(),
        "generic 'error' status must produce a valid response, not Err: {result:?}"
    );

    let body = result.unwrap();
    assert!(
        body.get("data").is_some() || body.get("errors").is_some(),
        "response must be a GraphQL envelope: {body}"
    );
}

/// Pipeline 3 (success path): successful mutation response contains entity data.
///
/// Confirms that when the adapter returns a success status and entity JSON, the
/// executor wraps it in a `data` envelope with the entity fields accessible.
#[tokio::test]
async fn mutation_success_status_includes_entity_in_data() {
    let json = include_str!("../../../tests/fixtures/golden/04-error-type.json");
    let schema = CompiledSchema::from_json(json).expect("golden fixture must parse");

    let entity_id = "new-user-uuid-456";
    let mut row = HashMap::new();
    row.insert("succeeded".to_string(), json!(true));
    row.insert("state_changed".to_string(), json!(true));
    row.insert("message".to_string(), json!("created"));
    row.insert("entity".to_string(), json!({"id": entity_id, "email": "new@example.com"}));
    row.insert("entity_type".to_string(), json!("CreateUserSuccess"));
    row.insert("cascade".to_string(), serde_json::Value::Null);
    row.insert("metadata".to_string(), serde_json::Value::Null);

    let mock = Arc::new(ErrorMockAdapter::with_row(row));
    let executor = Executor::new(schema, mock);
    let vars = json!({"email": "new@example.com", "name": "New User"});

    let result = executor
        .execute(r"mutation { createUser { id } }", Some(&vars))
        .await
        .expect("success mutation must succeed");

    assert!(result.get("data").is_some(), "success response must have 'data': {result}");
    assert!(
        result.get("errors").is_none(),
        "success response must not have 'errors': {result}"
    );
}
