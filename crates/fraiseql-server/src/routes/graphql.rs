//! GraphQL HTTP endpoint.

use axum::{
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use fraiseql_core::{db::traits::DatabaseAdapter, runtime::Executor};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::Instant;
use tracing::{debug, error, info};

/// GraphQL request payload.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GraphQLRequest {
    /// GraphQL query string.
    pub query: String,

    /// Query variables (optional).
    #[serde(default)]
    pub variables: Option<serde_json::Value>,

    /// Operation name (optional).
    #[serde(default)]
    pub operation_name: Option<String>,
}

/// GraphQL response payload.
#[derive(Debug, Serialize)]
pub struct GraphQLResponse {
    /// Response data or errors.
    #[serde(flatten)]
    pub body: serde_json::Value,
}

impl IntoResponse for GraphQLResponse {
    fn into_response(self) -> Response {
        Json(self.body).into_response()
    }
}

/// Server state containing executor and configuration.
#[derive(Clone)]
pub struct AppState<A: DatabaseAdapter> {
    /// Query executor.
    pub executor: Arc<Executor<A>>,
}

impl<A: DatabaseAdapter> AppState<A> {
    /// Create new application state.
    #[must_use]
    pub fn new(executor: Arc<Executor<A>>) -> Self {
        Self { executor }
    }
}

/// GraphQL HTTP handler.
///
/// Handles POST requests to the GraphQL endpoint:
/// 1. Parse GraphQL request body
/// 2. Execute query via Executor
/// 3. Return GraphQL response
///
/// Tracks execution timing and operation name for monitoring.
/// Provides detailed error information with appropriate HTTP status codes.
///
/// # Errors
///
/// Returns HTTP 400 for invalid requests or query errors.
/// Returns HTTP 500 for internal server errors.
pub async fn graphql_handler<A: DatabaseAdapter + Clone + Send + Sync + 'static>(
    State(state): State<AppState<A>>,
    Json(request): Json<GraphQLRequest>,
) -> Result<GraphQLResponse, GraphQLError> {
    let start_time = Instant::now();

    info!(
        query_length = request.query.len(),
        has_variables = request.variables.is_some(),
        operation_name = ?request.operation_name,
        "Executing GraphQL query"
    );

    // Execute query
    let result = state
        .executor
        .execute(&request.query, request.variables.as_ref())
        .await
        .map_err(|e| {
            let elapsed = start_time.elapsed();
            error!(
                error = %e,
                elapsed_ms = elapsed.as_millis(),
                operation_name = ?request.operation_name,
                "Query execution failed"
            );
            GraphQLError::ExecutionError(e.to_string())
        })?;

    let elapsed = start_time.elapsed();
    debug!(
        response_length = result.len(),
        elapsed_ms = elapsed.as_millis(),
        operation_name = ?request.operation_name,
        "Query executed successfully"
    );

    // Parse result as JSON
    let response_json: serde_json::Value = serde_json::from_str(&result)
        .map_err(|e| {
            error!(
                error = %e,
                response_length = result.len(),
                "Failed to deserialize executor response"
            );
            GraphQLError::SerializationError(e.to_string())
        })?;

    Ok(GraphQLResponse {
        body: response_json,
    })
}

/// GraphQL error type.
#[derive(Debug, thiserror::Error)]
pub enum GraphQLError {
    /// Query execution error.
    #[error("Execution error: {0}")]
    ExecutionError(String),

    /// Serialization error.
    #[error("Serialization error: {0}")]
    SerializationError(String),
}

impl IntoResponse for GraphQLError {
    fn into_response(self) -> Response {
        let (status, error_message) = match self {
            Self::ExecutionError(msg) => (StatusCode::BAD_REQUEST, msg),
            Self::SerializationError(msg) => (StatusCode::INTERNAL_SERVER_ERROR, msg),
        };

        let body = serde_json::json!({
            "errors": [{
                "message": error_message
            }]
        });

        (status, Json(body)).into_response()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_graphql_request_deserialize() {
        let json = r#"{"query": "{ users { id } }"}"#;
        let request: GraphQLRequest = serde_json::from_str(json).unwrap();
        assert_eq!(request.query, "{ users { id } }");
        assert!(request.variables.is_none());
    }

    #[test]
    fn test_graphql_request_with_variables() {
        let json = r#"{"query": "query($id: ID!) { user(id: $id) { name } }", "variables": {"id": "123"}}"#;
        let request: GraphQLRequest = serde_json::from_str(json).unwrap();
        assert!(request.variables.is_some());
    }
}
