//! GraphQL HTTP endpoint.

use axum::{
    extract::State,
    response::{IntoResponse, Response},
    Json,
};
use fraiseql_core::{db::traits::DatabaseAdapter, runtime::Executor};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::Instant;
use tracing::{debug, error, info};

use crate::error::{ErrorResponse, GraphQLError};
use crate::validation::RequestValidator;

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
/// 1. Validate GraphQL request (depth, complexity)
/// 2. Parse GraphQL request body
/// 3. Execute query via Executor
/// 4. Return GraphQL response with proper error formatting
///
/// Tracks execution timing and operation name for monitoring.
/// Provides GraphQL spec-compliant error responses.
///
/// # Errors
///
/// Returns appropriate HTTP status codes based on error type.
pub async fn graphql_handler<A: DatabaseAdapter + Clone + Send + Sync + 'static>(
    State(state): State<AppState<A>>,
    Json(request): Json<GraphQLRequest>,
) -> Result<GraphQLResponse, ErrorResponse> {
    let start_time = Instant::now();

    info!(
        query_length = request.query.len(),
        has_variables = request.variables.is_some(),
        operation_name = ?request.operation_name,
        "Executing GraphQL query"
    );

    // Validate request
    let validator = RequestValidator::new();

    // Validate query
    if let Err(e) = validator.validate_query(&request.query) {
        error!(
            error = %e,
            operation_name = ?request.operation_name,
            "Query validation failed"
        );
        let graphql_error = match e {
            crate::validation::ValidationError::QueryTooDeep {
                max_depth,
                actual_depth,
            } => GraphQLError::validation(format!(
                "Query exceeds maximum depth: {actual_depth} > {max_depth}"
            )),
            crate::validation::ValidationError::QueryTooComplex {
                max_complexity,
                actual_complexity,
            } => GraphQLError::validation(format!(
                "Query exceeds maximum complexity: {actual_complexity} > {max_complexity}"
            )),
            crate::validation::ValidationError::MalformedQuery(msg) => {
                GraphQLError::parse(msg)
            }
            crate::validation::ValidationError::InvalidVariables(msg) => {
                GraphQLError::request(msg)
            }
        };
        return Err(ErrorResponse::from_error(graphql_error));
    }

    // Validate variables
    if let Err(e) = validator.validate_variables(request.variables.as_ref()) {
        error!(
            error = %e,
            operation_name = ?request.operation_name,
            "Variables validation failed"
        );
        return Err(ErrorResponse::from_error(GraphQLError::request(e.to_string())));
    }

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
            ErrorResponse::from_error(GraphQLError::execution(&e.to_string()))
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
            ErrorResponse::from_error(GraphQLError::internal(format!(
                "Failed to process response: {e}"
            )))
        })?;

    Ok(GraphQLResponse {
        body: response_json,
    })
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
