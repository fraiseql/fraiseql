//! GraphQL HTTP endpoint.
//!
//! Supports both POST and GET requests per the GraphQL over HTTP spec:
//! - POST: JSON body with `query`, `variables`, `operationName`
//! - GET: Query parameters `query`, `variables` (JSON-encoded), `operationName`

use std::{
    sync::{Arc, atomic::Ordering},
    time::Instant,
};

use axum::{
    Json,
    extract::{Query, State},
    http::HeaderMap,
    response::{IntoResponse, Response},
};
use fraiseql_core::{db::traits::DatabaseAdapter, runtime::Executor};
use serde::{Deserialize, Serialize};
use tracing::{debug, error, info, warn};

use crate::{
    error::{ErrorResponse, GraphQLError},
    middleware::AuthUser,
    metrics_server::MetricsCollector,
    tracing_utils,
    validation::RequestValidator,
};
use fraiseql_core::security::SecurityContext;

/// GraphQL request payload (for POST requests).
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

/// GraphQL GET request parameters.
///
/// Per GraphQL over HTTP spec, GET requests encode parameters in the query string:
/// - `query`: Required, the GraphQL query string
/// - `variables`: Optional, JSON-encoded object
/// - `operationName`: Optional, name of the operation to execute
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GraphQLGetParams {
    /// GraphQL query string (required).
    pub query: String,

    /// Query variables as JSON-encoded string (optional).
    #[serde(default)]
    pub variables: Option<String>,

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
    /// Metrics collector.
    pub metrics:  Arc<MetricsCollector>,
}

impl<A: DatabaseAdapter> AppState<A> {
    /// Create new application state.
    #[must_use]
    pub fn new(executor: Arc<Executor<A>>) -> Self {
        Self {
            executor,
            metrics: Arc::new(MetricsCollector::new()),
        }
    }

    /// Create new application state with custom metrics collector.
    #[must_use]
    pub fn with_metrics(executor: Arc<Executor<A>>, metrics: Arc<MetricsCollector>) -> Self {
        Self { executor, metrics }
    }
}

/// GraphQL HTTP handler for POST requests.
///
/// Handles POST requests to the GraphQL endpoint:
/// 1. Extract W3C trace context from traceparent header (if present)
/// 2. Validate GraphQL request (depth, complexity)
/// 3. Parse GraphQL request body
/// 4. Execute query via Executor with optional SecurityContext
/// 5. Return GraphQL response with proper error formatting
///
/// Tracks execution timing and operation name for monitoring.
/// Provides GraphQL spec-compliant error responses.
/// Supports W3C Trace Context for distributed tracing.
/// Supports OIDC authentication for RLS policy evaluation.
///
/// # Errors
///
/// Returns appropriate HTTP status codes based on error type.
pub async fn graphql_handler<A: DatabaseAdapter + Clone + Send + Sync + 'static>(
    State(state): State<AppState<A>>,
    headers: HeaderMap,
    Json(request): Json<GraphQLRequest>,
) -> Result<GraphQLResponse, ErrorResponse> {
    // Extract trace context from W3C headers
    let trace_context = tracing_utils::extract_trace_context(&headers);
    if trace_context.is_some() {
        debug!("Extracted W3C trace context from incoming request");
    }

    // Create security context from request headers (user would be in extensions if auth middleware ran)
    // For now, create basic context without auth user
    let security_context = None;

    execute_graphql_request(state, request, trace_context, security_context).await
}

/// GraphQL HTTP handler for GET requests.
///
/// Handles GET requests to the GraphQL endpoint per the GraphQL over HTTP spec.
/// Query parameters:
/// - `query`: Required, the GraphQL query string (URL-encoded)
/// - `variables`: Optional, JSON-encoded variables object (URL-encoded)
/// - `operationName`: Optional, name of the operation to execute
///
/// Supports W3C Trace Context via traceparent header for distributed tracing.
///
/// Example:
/// ```text
/// GET /graphql?query={users{id,name}}&variables={"limit":10}
/// ```
///
/// # Errors
///
/// Returns appropriate HTTP status codes based on error type.
///
/// # Note
///
/// Per GraphQL over HTTP spec, GET requests should only be used for queries,
/// not mutations (which should use POST). This handler does not enforce that
/// restriction but logs a warning for mutation-like queries.
pub async fn graphql_get_handler<A: DatabaseAdapter + Clone + Send + Sync + 'static>(
    State(state): State<AppState<A>>,
    headers: HeaderMap,
    Query(params): Query<GraphQLGetParams>,
) -> Result<GraphQLResponse, ErrorResponse> {
    // Parse variables from JSON string
    let variables = if let Some(vars_str) = params.variables {
        match serde_json::from_str::<serde_json::Value>(&vars_str) {
            Ok(v) => Some(v),
            Err(e) => {
                warn!(
                    error = %e,
                    variables = %vars_str,
                    "Failed to parse variables JSON in GET request"
                );
                return Err(ErrorResponse::from_error(GraphQLError::request(format!(
                    "Invalid variables JSON: {e}"
                ))));
            },
        }
    } else {
        None
    };

    // Warn if this looks like a mutation (GET should be for queries only)
    if params.query.trim_start().starts_with("mutation") {
        warn!(
            operation_name = ?params.operation_name,
            "Mutation sent via GET request - should use POST"
        );
    }

    let trace_context = tracing_utils::extract_trace_context(&headers);
    if trace_context.is_some() {
        debug!("Extracted W3C trace context from incoming request");
    }

    let request = GraphQLRequest {
        query: params.query,
        variables,
        operation_name: params.operation_name,
    };

    // NOTE: SecurityContext extraction will be handled via middleware in next iteration
    // For now, execute without security context
    execute_graphql_request(state, request, trace_context, None).await
}

/// Create SecurityContext from authenticated user and request headers.
#[allow(dead_code)]
fn create_security_context(
    auth_user: Option<AuthUser>,
    headers: &HeaderMap,
) -> Option<SecurityContext> {
    auth_user.map(|auth_user| {
        let authenticated_user = auth_user.0;
        let request_id = extract_request_id(headers);
        let ip_address = extract_ip_address(headers);
        let tenant_id = extract_tenant_id(headers);

        let mut context = SecurityContext::from_user(authenticated_user, request_id);
        context.ip_address = ip_address;
        context.tenant_id = tenant_id;
        context
    })
}

/// Extract request ID from headers or generate a new one.
fn extract_request_id(headers: &HeaderMap) -> String {
    headers
        .get("x-request-id")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string())
        .unwrap_or_else(|| format!("req-{}", uuid::Uuid::new_v4()))
}

/// Extract client IP address from headers.
fn extract_ip_address(headers: &HeaderMap) -> Option<String> {
    // Check X-Forwarded-For first (for proxied requests)
    if let Some(forwarded_for) = headers
        .get("x-forwarded-for")
        .and_then(|v| v.to_str().ok())
    {
        // X-Forwarded-For can contain multiple IPs, use the first one
        return forwarded_for.split(',').next().map(|ip| ip.trim().to_string());
    }

    // Check X-Real-IP
    if let Some(real_ip) = headers
        .get("x-real-ip")
        .and_then(|v| v.to_str().ok())
    {
        return Some(real_ip.to_string());
    }

    None
}

/// Extract tenant ID from headers.
fn extract_tenant_id(headers: &HeaderMap) -> Option<String> {
    headers
        .get("x-tenant-id")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string())
}

/// Shared GraphQL execution logic for both GET and POST handlers.
async fn execute_graphql_request<A: DatabaseAdapter + Clone + Send + Sync + 'static>(
    state: AppState<A>,
    request: GraphQLRequest,
    _trace_context: Option<fraiseql_core::federation::FederationTraceContext>,
    security_context: Option<SecurityContext>,
) -> Result<GraphQLResponse, ErrorResponse> {
    let start_time = Instant::now();
    let metrics = &state.metrics;

    // Increment total queries counter
    metrics.queries_total.fetch_add(1, Ordering::Relaxed);

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
        metrics.queries_error.fetch_add(1, Ordering::Relaxed);
        metrics.validation_errors_total.fetch_add(1, Ordering::Relaxed);
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
                metrics.parse_errors_total.fetch_add(1, Ordering::Relaxed);
                GraphQLError::parse(msg)
            },
            crate::validation::ValidationError::InvalidVariables(msg) => GraphQLError::request(msg),
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
        metrics.queries_error.fetch_add(1, Ordering::Relaxed);
        metrics.validation_errors_total.fetch_add(1, Ordering::Relaxed);
        return Err(ErrorResponse::from_error(GraphQLError::request(e.to_string())));
    }

    // Execute query with or without security context
    let result = if let Some(sec_ctx) = security_context {
        state
            .executor
            .execute_with_security(&request.query, request.variables.as_ref(), &sec_ctx)
            .await
            .map_err(|e| {
                let elapsed = start_time.elapsed();
                error!(
                    error = %e,
                    elapsed_ms = elapsed.as_millis(),
                    operation_name = ?request.operation_name,
                    "Query execution failed"
                );
                metrics.queries_error.fetch_add(1, Ordering::Relaxed);
                metrics.execution_errors_total.fetch_add(1, Ordering::Relaxed);
                // Record duration even for failed queries
                metrics
                    .queries_duration_us
                    .fetch_add(elapsed.as_micros() as u64, Ordering::Relaxed);
                ErrorResponse::from_error(GraphQLError::execution(&e.to_string()))
            })?
    } else {
        state
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
                metrics.queries_error.fetch_add(1, Ordering::Relaxed);
                metrics.execution_errors_total.fetch_add(1, Ordering::Relaxed);
                // Record duration even for failed queries
                metrics
                    .queries_duration_us
                    .fetch_add(elapsed.as_micros() as u64, Ordering::Relaxed);
                ErrorResponse::from_error(GraphQLError::execution(&e.to_string()))
            })?
    };

    let elapsed = start_time.elapsed();
    let elapsed_us = elapsed.as_micros() as u64;

    // Record successful query metrics
    metrics.queries_success.fetch_add(1, Ordering::Relaxed);
    metrics.queries_duration_us.fetch_add(elapsed_us, Ordering::Relaxed);
    metrics.db_queries_total.fetch_add(1, Ordering::Relaxed);
    metrics.db_queries_duration_us.fetch_add(elapsed_us, Ordering::Relaxed);

    // Record federation-specific metrics for federation queries
    if fraiseql_core::federation::is_federation_query(&request.query) {
        metrics.record_entity_resolution(elapsed_us, true);
    }

    debug!(
        response_length = result.len(),
        elapsed_ms = elapsed.as_millis(),
        operation_name = ?request.operation_name,
        "Query executed successfully"
    );

    // Parse result as JSON
    let response_json: serde_json::Value = serde_json::from_str(&result).map_err(|e| {
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

    #[test]
    fn test_graphql_get_params_deserialize() {
        // Simulate URL query params: ?query={users{id}}&operationName=GetUsers
        let params: GraphQLGetParams = serde_json::from_value(serde_json::json!({
            "query": "{ users { id } }",
            "operationName": "GetUsers"
        }))
        .unwrap();

        assert_eq!(params.query, "{ users { id } }");
        assert_eq!(params.operation_name, Some("GetUsers".to_string()));
        assert!(params.variables.is_none());
    }

    #[test]
    fn test_graphql_get_params_with_variables() {
        // Variables should be JSON-encoded string in GET requests
        let params: GraphQLGetParams = serde_json::from_value(serde_json::json!({
            "query": "query($id: ID!) { user(id: $id) { name } }",
            "variables": r#"{"id": "123"}"#
        }))
        .unwrap();

        assert!(params.variables.is_some());
        let vars_str = params.variables.unwrap();
        let vars: serde_json::Value = serde_json::from_str(&vars_str).unwrap();
        assert_eq!(vars["id"], "123");
    }

    #[test]
    fn test_graphql_get_params_camel_case() {
        // Test camelCase field names
        let params: GraphQLGetParams = serde_json::from_value(serde_json::json!({
            "query": "{ users { id } }",
            "operationName": "TestOp"
        }))
        .unwrap();

        assert_eq!(params.operation_name, Some("TestOp".to_string()));
    }

    #[test]
    fn test_extract_request_id_from_header() {
        let mut headers = axum::http::HeaderMap::new();
        headers.insert("x-request-id", "req-12345".parse().unwrap());

        let request_id = extract_request_id(&headers);
        assert_eq!(request_id, "req-12345");
    }

    #[test]
    fn test_extract_request_id_generates_default() {
        let headers = axum::http::HeaderMap::new();
        let request_id = extract_request_id(&headers);
        // Should start with "req-"
        assert!(request_id.starts_with("req-"));
        // Should contain a UUID: "req-" (4) + UUID (36) = 40 chars
        assert_eq!(request_id.len(), 40);
    }

    #[test]
    fn test_extract_ip_address_from_x_forwarded_for() {
        let mut headers = axum::http::HeaderMap::new();
        headers.insert("x-forwarded-for", "192.0.2.1, 10.0.0.1".parse().unwrap());

        let ip = extract_ip_address(&headers);
        assert_eq!(ip, Some("192.0.2.1".to_string()));
    }

    #[test]
    fn test_extract_ip_address_from_x_real_ip() {
        let mut headers = axum::http::HeaderMap::new();
        headers.insert("x-real-ip", "10.0.0.2".parse().unwrap());

        let ip = extract_ip_address(&headers);
        assert_eq!(ip, Some("10.0.0.2".to_string()));
    }

    #[test]
    fn test_extract_ip_address_none_when_missing() {
        let headers = axum::http::HeaderMap::new();
        let ip = extract_ip_address(&headers);
        assert_eq!(ip, None);
    }

    #[test]
    fn test_extract_tenant_id_from_header() {
        let mut headers = axum::http::HeaderMap::new();
        headers.insert("x-tenant-id", "tenant-acme".parse().unwrap());

        let tenant_id = extract_tenant_id(&headers);
        assert_eq!(tenant_id, Some("tenant-acme".to_string()));
    }

    #[test]
    fn test_extract_tenant_id_none_when_missing() {
        let headers = axum::http::HeaderMap::new();
        let tenant_id = extract_tenant_id(&headers);
        assert_eq!(tenant_id, None);
    }

    // RED: Test demonstrating desired Cycle 4 behavior
    // This test will pass once we properly wire SecurityContext extraction from auth middleware
    #[test]
    fn test_security_context_creation_from_auth_user() {
        use chrono::Utc;

        // Simulate an authenticated user from the OIDC middleware
        let auth_user = crate::middleware::AuthUser(fraiseql_core::security::AuthenticatedUser {
            user_id: "user123".to_string(),
            scopes: vec!["read:user".to_string(), "write:post".to_string()],
            expires_at: Utc::now() + chrono::Duration::hours(1),
        });

        // Create headers with additional metadata
        let mut headers = axum::http::HeaderMap::new();
        headers.insert("x-request-id", "req-test-123".parse().unwrap());
        headers.insert("x-tenant-id", "tenant-acme".parse().unwrap());
        headers.insert("x-forwarded-for", "192.0.2.100".parse().unwrap());

        // Create security context
        let context = create_security_context(Some(auth_user), &headers);

        // Verify context was created correctly
        assert!(context.is_some());
        let sec_ctx = context.unwrap();
        assert_eq!(sec_ctx.user_id, "user123");
        assert_eq!(sec_ctx.scopes, vec!["read:user".to_string(), "write:post".to_string()]);
        assert_eq!(sec_ctx.tenant_id, Some("tenant-acme".to_string()));
        assert_eq!(sec_ctx.request_id, "req-test-123");
        assert_eq!(sec_ctx.ip_address, Some("192.0.2.100".to_string()));
    }
}
