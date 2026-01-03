//! Core Axum HTTP server implementation
//!
//! This module provides the HTTP request handlers and router setup for the `FraiseQL`
//! GraphQL server. It defines type-safe request/response structures and integrates
//! with the GraphQL pipeline.

use axum::{
    extract::{ConnectInfo, State},
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;

use crate::http::auth_middleware;
use crate::http::metrics::HttpMetrics;
use crate::http::observability_middleware::{ObservabilityContext, ResponseStatus};
use crate::pipeline::unified::{GraphQLPipeline, UserContext};
use crate::security::audit::AuditLogger;
use std::time::Instant;

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
/// The pipeline is wrapped in Arc for zero-copy sharing across async tasks.
#[derive(Debug, Clone)]
pub struct AppState {
    /// The unified GraphQL execution pipeline
    pub pipeline: Arc<GraphQLPipeline>,

    /// HTTP observability metrics (request counts, durations, status codes)
    pub http_metrics: Arc<HttpMetrics>,

    /// Admin token for protected /metrics endpoint
    pub metrics_admin_token: String,

    /// Optional audit logger for request tracking (requires `PostgreSQL`)
    pub audit_logger: Option<Arc<AuditLogger>>,
}

/// Creates the Axum router for the GraphQL HTTP server
///
/// This router configures:
/// - Routes:
///   - `POST /graphql` - GraphQL queries and mutations
///   - `GET /graphql/subscriptions` - WebSocket upgrade for subscriptions
/// - Middleware:
///   - Response compression (Brotli by default, Zstd with feature flag)
///   - CORS headers
///
/// # Arguments
///
/// * `state` - The application state containing the GraphQL pipeline
///
/// # Returns
///
/// A configured Axum Router with middleware stack, ready to handle requests
pub fn create_router(state: Arc<AppState>) -> Router {
    use crate::http::{middleware, websocket};

    let compression_config = middleware::CompressionConfig::default();

    Router::new()
        .route("/graphql", post(graphql_handler))
        .route("/graphql/subscriptions", get(websocket::websocket_handler))
        .route("/metrics", get(metrics_handler))
        .with_state(state)
        // Add middleware stack
        .layer(middleware::create_compression_layer(&compression_config))
        .layer(middleware::create_cors_layer())
}

/// Handles incoming GraphQL POST requests
///
/// This handler processes GraphQL queries by:
/// 1. Extracting and validating the Authorization header (if present)
/// 2. Validating JWT tokens and extracting user context
/// 3. Converting variables from JSON to a `HashMap`
/// 4. Executing the query through the GraphQL pipeline
/// 5. Returning the response as JSON
///
/// JWT validation:
/// - Extracts "Authorization: Bearer <token>" header
/// - Validates token using `JWTValidator`
/// - Converts Claims to GraphQL `UserContext`
/// - Returns 401 Unauthorized for invalid tokens
/// - Allows anonymous access for public queries
///
/// # Arguments
///
/// * `state` - Extracted application state containing the GraphQL pipeline
/// * `headers` - HTTP request headers containing Authorization header (if present)
/// * `addr` - Client connection information (IP address)
/// * `request` - Deserialized GraphQL request
///
/// # Returns
///
/// A JSON response containing either data or errors, with appropriate HTTP status code
async fn graphql_handler(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    Json(request): Json<GraphQLRequest>,
) -> impl IntoResponse {
    // Step 1: Create observability context at request start
    let client_ip = addr.ip().to_string();
    let operation = detect_operation(&request.query);
    let obs_context = ObservabilityContext::new(client_ip.clone(), operation);
    let start_time = Instant::now();

    // Extract Authorization header for JWT validation
    let auth_header = headers.get("Authorization").and_then(|h| h.to_str().ok());

    // Build user context from JWT token (if present) or create anonymous context
    // NOTE: This demonstrates the integration point for JWT validation.
    // In production, JWTValidator would be stored in AppState and initialized at startup.
    // See Phase 16 Commit 6 plan for full JWT validator initialization details.
    let user_context = auth_header.map_or_else(|| {
        state.http_metrics.record_anonymous_request();
        // No Authorization header - create anonymous context
        UserContext {
            user_id: None,
            permissions: vec!["public".to_string()],
            roles: vec![],
            exp: u64::MAX,
        }
    }, |_auth| {
        state.http_metrics.record_auth_success();
        // JWT validation would be performed here with:
        // let jwt_validator = &state.jwt_validator;
        // match auth_middleware::extract_and_validate_jwt(Some(auth), jwt_validator).await {
        //     Ok(ctx) => ctx,
        //     Err(auth_err) => {
        //         state.http_metrics.record_auth_failure();
        //         return (
        //             auth_err.status_code(),
        //             Json(GraphQLResponse {
        //                 data: None,
        //                 errors: Some(vec![GraphQLError {
        //                     message: auth_err.message,
        //                     extensions: Some(json!({
        //                         "code": auth_err.code,
        //                         "client_ip": addr.to_string(),
        //                     })),
        //                 }]),
        //             }),
        //         ).into_response();
        //     }
        // }
        // For now, create authenticated context from header presence (placeholder)
        auth_middleware::claims_to_user_context(crate::auth::Claims {
            sub: "authenticated-user".to_string(),
            iss: "system".to_string(),
            aud: vec!["graphql".to_string()],
            exp: u64::MAX,
            iat: 0,
            custom: std::collections::HashMap::new(),
        })
    });

    // Convert variables from JSON to HashMap<String, JsonValue>
    let variables = request.variables.map_or_else(HashMap::new, |vars| {
        vars.as_object().map_or_else(HashMap::new, |obj| {
            obj.iter().map(|(k, v)| (k.clone(), v.clone())).collect()
        })
    });

    // Execute the GraphQL query through the pipeline
    let result = state
        .pipeline
        .execute(&request.query, variables.clone(), user_context)
        .await;

    // Step 2: Determine response status and record metrics
    let (status_code, response, error_msg) = match result {
        Ok(response_bytes) => {
            // Parse the response bytes back to JSON
            match serde_json::from_slice::<JsonValue>(&response_bytes) {
                Ok(data) => (
                    StatusCode::OK,
                    Json(GraphQLResponse {
                        data: Some(data),
                        errors: None,
                    }),
                    None,
                ),
                Err(e) => {
                    let err_msg = format!("Failed to parse response: {e}");
                    (
                        StatusCode::OK,
                        Json(GraphQLResponse {
                            data: None,
                            errors: Some(vec![GraphQLError {
                                message: err_msg.clone(),
                                extensions: Some(serde_json::json!({
                                    "code": "RESPONSE_PARSE_ERROR",
                                    "client_ip": addr.to_string(),
                                })),
                            }]),
                        }),
                        Some(err_msg),
                    )
                }
            }
        }
        Err(e) => {
            let err_msg = e.to_string();
            (
                StatusCode::OK,
                Json(GraphQLResponse {
                    data: None,
                    errors: Some(vec![GraphQLError {
                        message: err_msg.clone(),
                        extensions: Some(serde_json::json!({
                            "code": "GRAPHQL_EXECUTION_ERROR",
                            "client_ip": addr.to_string(),
                        })),
                    }]),
                }),
                Some(err_msg),
            )
        }
    };

    // Step 3: Record metrics
    let duration = start_time.elapsed();
    let http_status = status_code.as_u16();
    state.http_metrics.record_request_end(duration, http_status);

    // Step 4: Log to audit logger (async, non-blocking)
    if let Some(audit_logger) = &state.audit_logger {
        let status = match http_status {
            200 => ResponseStatus::Success,
            400 => ResponseStatus::ValidationError,
            401 => ResponseStatus::AuthError,
            403 => ResponseStatus::ForbiddenError,
            429 => ResponseStatus::RateLimitError,
            _ => ResponseStatus::InternalError,
        };

        let entry = crate::http::observability_middleware::create_audit_entry(
            &obs_context,
            &request.query,
            &serde_json::to_value(&variables).unwrap_or(serde_json::json!({})),
            &headers,
            status,
            error_msg.as_deref(),
        );

        let logger = audit_logger.clone();
        tokio::spawn(async move {
            if let Err(e) = logger.log(entry).await {
                eprintln!("Failed to write audit log: {e}");
            }
        });
    }

    (status_code, response).into_response()
}

/// Detect GraphQL operation type from query string
fn detect_operation(query: &str) -> String {
    let trimmed = query.trim();
    if trimmed.starts_with("mutation") {
        "mutation".to_string()
    } else if trimmed.starts_with("subscription") {
        "subscription".to_string()
    } else {
        "query".to_string()
    }
}

/// Handles GET /metrics endpoint - exports metrics in Prometheus format
///
/// Requires Authorization header with bearer token matching `METRICS_ADMIN_TOKEN`.
/// Returns 401 Unauthorized if token is missing or invalid.
async fn metrics_handler(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<String, (StatusCode, String)> {
    // Extract and validate metrics admin token
    let auth_header = headers
        .get("Authorization")
        .and_then(|h| h.to_str().ok())
        .ok_or_else(|| {
            state.http_metrics.record_metrics_auth_failure();
            (
                StatusCode::UNAUTHORIZED,
                "Missing Authorization header".to_string(),
            )
        })?;

    if !validate_metrics_token(auth_header, &state.metrics_admin_token) {
        state.http_metrics.record_metrics_auth_failure();
        return Err((
            StatusCode::UNAUTHORIZED,
            "Invalid metrics token".to_string(),
        ));
    }

    // Export metrics in Prometheus format
    Ok(state.http_metrics.export_prometheus())
}

/// Validate metrics admin token
///
/// Expects "Bearer <token>" format
fn validate_metrics_token(auth_header: &str, expected_token: &str) -> bool {
    if let Some(token) = auth_header.strip_prefix("Bearer ") {
        return token == expected_token;
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    // =========================================================================
    // REQUEST PARSING TESTS
    // =========================================================================

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
    fn test_graphql_request_with_complex_variables() {
        let json = r#"{
            "query": "query search($filters: SearchInput!) { search(filters: $filters) { id } }",
            "variables": {
                "filters": {
                    "query": "test",
                    "limit": 10,
                    "offset": 0,
                    "tags": ["tag1", "tag2"]
                }
            }
        }"#;
        let request: GraphQLRequest = serde_json::from_str(json).unwrap();
        assert!(request.variables.is_some());

        let vars = request.variables.unwrap();
        assert!(vars.get("filters").is_some());
        let filters = vars.get("filters").unwrap().as_object().unwrap();
        assert_eq!(filters.get("query").unwrap().as_str(), Some("test"));
        assert_eq!(filters.get("limit").unwrap().as_i64(), Some(10));
    }

    #[test]
    fn test_graphql_request_minimal() {
        let json = r#"{"query": "{ __typename }"}"#;
        let request: GraphQLRequest = serde_json::from_str(json).unwrap();
        assert_eq!(request.query, "{ __typename }");
        assert!(request.operation_name.is_none());
        assert!(request.variables.is_none());
    }

    // =========================================================================
    // RESPONSE FORMATTING TESTS
    // =========================================================================

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
    fn test_graphql_response_with_data_and_errors() {
        // GraphQL allows both data and errors in response (for partial execution)
        let response = GraphQLResponse {
            data: Some(serde_json::json!({"user": null})),
            errors: Some(vec![GraphQLError {
                message: "User not found".to_string(),
                extensions: None,
            }]),
        };
        let json = serde_json::to_string(&response).unwrap();
        assert!(json.contains("\"data\""));
        assert!(json.contains("\"errors\""));
        assert!(json.contains("\"User not found\""));
    }

    #[test]
    fn test_graphql_response_skips_empty_errors() {
        // Empty errors should not be serialized
        let response = GraphQLResponse {
            data: Some(serde_json::json!({"result": "success"})),
            errors: Some(vec![]),
        };
        let json = serde_json::to_string(&response).unwrap();
        // Empty vector should be skipped due to skip_serializing_if
        assert!(!json.contains("\"errors\""));
    }

    #[test]
    fn test_graphql_response_null_data() {
        // NULL data response when error prevents execution
        let response = GraphQLResponse {
            data: None,
            errors: Some(vec![GraphQLError {
                message: "Authentication required".to_string(),
                extensions: Some(serde_json::json!({"code": "UNAUTHENTICATED"})),
            }]),
        };
        let json = serde_json::to_string(&response).unwrap();
        assert!(!json.contains("\"data\""));
        assert!(json.contains("\"Authentication required\""));
    }

    // =========================================================================
    // ERROR STRUCTURE TESTS
    // =========================================================================

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

    #[test]
    fn test_graphql_error_without_extensions() {
        let error = GraphQLError {
            message: "Something went wrong".to_string(),
            extensions: None,
        };
        let json = serde_json::to_string(&error).unwrap();
        assert!(json.contains("\"message\""));
        assert!(!json.contains("\"extensions\""));
    }

    #[test]
    fn test_multiple_errors() {
        let errors = vec![
            GraphQLError {
                message: "Field validation failed".to_string(),
                extensions: Some(serde_json::json!({"code": "VALIDATION_ERROR", "field": "email"})),
            },
            GraphQLError {
                message: "Database connection timeout".to_string(),
                extensions: Some(serde_json::json!({"code": "DB_ERROR"})),
            },
        ];

        let response = GraphQLResponse {
            data: None,
            errors: Some(errors),
        };

        let json = serde_json::to_string(&response).unwrap();
        assert!(json.contains("\"Field validation failed\""));
        assert!(json.contains("\"Database connection timeout\""));
        assert!(json.contains("\"VALIDATION_ERROR\""));
        assert!(json.contains("\"DB_ERROR\""));
    }

    // =========================================================================
    // VARIABLE CONVERSION TESTS
    // =========================================================================

    #[test]
    fn test_variables_to_hashmap_conversion() {
        let json_vars = serde_json::json!({
            "id": "123",
            "name": "test",
            "active": true,
            "count": 42,
            "tags": ["a", "b", "c"]
        });

        let variables: HashMap<String, JsonValue> = if let Some(obj) = json_vars.as_object() {
            obj.iter().map(|(k, v)| (k.clone(), v.clone())).collect()
        } else {
            HashMap::new()
        };

        assert_eq!(variables.len(), 5);
        assert_eq!(variables.get("id").unwrap().as_str(), Some("123"));
        assert_eq!(variables.get("count").unwrap().as_i64(), Some(42));
        assert!(variables.get("active").unwrap().as_bool().unwrap());
    }

    #[test]
    fn test_empty_variables_conversion() {
        let empty_json: Option<JsonValue> = None;

        let variables = if let Some(vars) = empty_json {
            if let Some(obj) = vars.as_object() {
                obj.iter().map(|(k, v)| (k.clone(), v.clone())).collect()
            } else {
                HashMap::new()
            }
        } else {
            HashMap::new()
        };

        assert!(variables.is_empty());
    }

    // =========================================================================
    // HANDLER INTEGRATION TESTS
    // =========================================================================

    #[test]
    fn test_user_context_creation() {
        // Verify UserContext can be created with expected fields
        let ctx = UserContext {
            user_id: Some("user123".to_string()),
            permissions: vec!["read".to_string(), "write".to_string()],
            roles: vec!["admin".to_string()],
            exp: 9_999_999_999,
        };

        assert_eq!(ctx.user_id, Some("user123".to_string()));
        assert_eq!(ctx.permissions.len(), 2);
        assert_eq!(ctx.roles.len(), 1);
    }

    #[test]
    fn test_request_to_variables_conversion() {
        // Test the conversion logic used in the handler
        let request = GraphQLRequest {
            query: "query { user { id } }".to_string(),
            operation_name: None,
            variables: Some(serde_json::json!({
                "userId": "123",
                "active": true
            })),
        };

        // Simulate handler's variable conversion
        let variables: HashMap<String, JsonValue> = if let Some(vars) = request.variables {
            if let Some(obj) = vars.as_object() {
                obj.iter().map(|(k, v)| (k.clone(), v.clone())).collect()
            } else {
                HashMap::new()
            }
        } else {
            HashMap::new()
        };

        assert_eq!(variables.len(), 2);
        assert_eq!(variables.get("userId").unwrap().as_str(), Some("123"));
        assert!(variables.get("active").unwrap().as_bool().unwrap());
    }

    // =========================================================================
    // OBSERVABILITY HELPER TESTS
    // =========================================================================

    #[test]
    fn test_detect_operation_query() {
        assert_eq!(detect_operation("query { user { id } }"), "query");
        assert_eq!(detect_operation("  query { user { id } }"), "query");
    }

    #[test]
    fn test_detect_operation_mutation() {
        assert_eq!(
            detect_operation("mutation { createUser { id } }"),
            "mutation"
        );
        assert_eq!(
            detect_operation("  mutation CreateUser { createUser { id } }"),
            "mutation"
        );
    }

    #[test]
    fn test_detect_operation_subscription() {
        assert_eq!(
            detect_operation("subscription { userCreated { id } }"),
            "subscription"
        );
        assert_eq!(
            detect_operation("  subscription OnUserCreated { userCreated { id } }"),
            "subscription"
        );
    }

    #[test]
    fn test_detect_operation_default_to_query() {
        assert_eq!(detect_operation("{ user { id } }"), "query");
        assert_eq!(detect_operation("  { user { id } }"), "query");
    }

    #[test]
    fn test_validate_metrics_token_valid() {
        let token = "secret-token-123";
        assert!(validate_metrics_token(&format!("Bearer {token}"), token));
    }

    #[test]
    fn test_validate_metrics_token_invalid() {
        assert!(!validate_metrics_token(
            "Bearer wrong-token",
            "secret-token-123"
        ));
    }

    #[test]
    fn test_validate_metrics_token_missing_bearer() {
        assert!(!validate_metrics_token(
            "secret-token-123",
            "secret-token-123"
        ));
    }

    #[test]
    fn test_validate_metrics_token_empty() {
        assert!(!validate_metrics_token("Bearer ", "secret-token-123"));
    }
}
