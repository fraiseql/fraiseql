//! Core Axum HTTP server implementation
//!
//! This module provides the HTTP request handlers and router setup for the FraiseQL
//! GraphQL server. It defines type-safe request/response structures and integrates
//! with the GraphQL pipeline.

use axum::{
    extract::{ConnectInfo, State},
    http::StatusCode,
    response::IntoResponse,
    routing::post,
    Json, Router,
};
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use std::sync::Arc;

/// GraphQL request structure
///
/// This structure represents an incoming GraphQL request with query, variables,
/// and optional operation name.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphQLRequest {
    /// The GraphQL query string
    pub query: String,

    /// Optional operation name if multiple operations are defined
    pub operation_name: Option<String>,

    /// Optional variables for the GraphQL operation
    #[serde(default)]
    pub variables: Option<serde_json::Value>,
}

/// GraphQL response structure
///
/// This structure represents the response returned by the GraphQL server.
/// It contains either the result data or error information.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphQLResponse {
    /// The data returned by the GraphQL operation (if successful)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,

    /// Errors that occurred during execution (if any)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub errors: Option<Vec<GraphQLError>>,
}

/// GraphQL error structure
///
/// Represents a single error that occurred during GraphQL execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphQLError {
    /// Error message
    pub message: String,

    /// Optional error extensions for additional context
    #[serde(skip_serializing_if = "Option::is_none")]
    pub extensions: Option<serde_json::Value>,
}

/// HTTP server state containing the GraphQL pipeline
///
/// This is shared across all request handlers via Axum's State mechanism.
#[derive(Debug)]
pub struct AppState {
    // TODO: Add GraphQL pipeline reference
    // This will be added in Commit 2
}

/// Creates the Axum router for the GraphQL HTTP server
///
/// # Arguments
///
/// * `state` - The application state containing the GraphQL pipeline
///
/// # Returns
///
/// A configured Axum Router ready to handle requests
pub fn create_router(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/graphql", post(graphql_handler))
        .with_state(state)
        .into()
}

/// Handles incoming GraphQL POST requests
///
/// # Arguments
///
/// * `state` - Extracted application state
/// * `addr` - Client connection information
/// * `request` - Deserialized GraphQL request
///
/// # Returns
///
/// A JSON response containing either data or errors
async fn graphql_handler(
    State(_state): State<Arc<AppState>>,
    ConnectInfo(_addr): ConnectInfo<SocketAddr>,
    Json(request): Json<GraphQLRequest>,
) -> impl IntoResponse {
    // TODO: Implement actual GraphQL execution
    // For now, return a placeholder response
    let response = GraphQLResponse {
        data: Some(serde_json::json!({
            "message": format!("Received query: {}", request.query)
        })),
        errors: None,
    };

    (StatusCode::OK, Json(response))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_graphql_request_serialization() {
        let json = r#"{"query": "{ test }"}"#;
        let request: GraphQLRequest = serde_json::from_str(json).unwrap();
        assert_eq!(request.query, "{ test }");
        assert!(request.operation_name.is_none());
        assert!(request.variables.is_none());
    }

    #[test]
    fn test_graphql_request_with_variables() {
        let json = r#"{
            "query": "query getUser($id: ID!) { user(id: $id) { name } }",
            "variables": {"id": "123"},
            "operationName": "getUser"
        }"#;
        let request: GraphQLRequest = serde_json::from_str(json).unwrap();
        assert_eq!(
            request.query,
            "query getUser($id: ID!) { user(id: $id) { name } }"
        );
        assert_eq!(request.operation_name, Some("getUser".to_string()));
        assert!(request.variables.is_some());
    }

    #[test]
    fn test_graphql_response_serialization() {
        let response = GraphQLResponse {
            data: Some(serde_json::json!({"user": {"id": "123", "name": "Alice"}})),
            errors: None,
        };
        let json = serde_json::to_string(&response).unwrap();
        assert!(json.contains("\"user\""));
        assert!(!json.contains("\"errors\""));
    }

    #[test]
    fn test_graphql_response_with_errors() {
        let response = GraphQLResponse {
            data: None,
            errors: Some(vec![GraphQLError {
                message: "Invalid query".to_string(),
                extensions: None,
            }]),
        };
        let json = serde_json::to_string(&response).unwrap();
        assert!(json.contains("\"errors\""));
        assert!(json.contains("\"Invalid query\""));
    }

    #[test]
    fn test_graphql_error_with_extensions() {
        let error = GraphQLError {
            message: "Parse error".to_string(),
            extensions: Some(serde_json::json!({
                "code": "GRAPHQL_PARSE_FAILED",
                "location": { "line": 1, "column": 5 }
            })),
        };
        let json = serde_json::to_string(&error).unwrap();
        assert!(json.contains("\"extensions\""));
        assert!(json.contains("\"GRAPHQL_PARSE_FAILED\""));
    }
}
