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
    metrics_server::MetricsCollector,
    tracing_utils,
    validation::RequestValidator,
};

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
/// 4. Execute query via Executor
/// 5. Return GraphQL response with proper error formatting
///
/// Tracks execution timing and operation name for monitoring.
/// Provides GraphQL spec-compliant error responses.
/// Supports W3C Trace Context for distributed tracing.
///
/// # Errors
///
/// Returns appropriate HTTP status codes based on error type.
pub async fn graphql_handler<A: DatabaseAdapter + Clone + Send + Sync + 'static>(
    State(state): State<AppState<A>>,
    headers: HeaderMap,
    Json(request): Json<GraphQLRequest>,
) -> Result<GraphQLResponse, ErrorResponse> {
    let trace_context = tracing_utils::extract_trace_context(&headers);
    if trace_context.is_some() {
        debug!("Extracted W3C trace context from incoming request");
    }
    execute_graphql_request(state, request, trace_context).await
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

    execute_graphql_request(state, request, trace_context).await
}

/// Shared GraphQL execution logic for both GET and POST handlers.
async fn execute_graphql_request<A: DatabaseAdapter + Clone + Send + Sync + 'static>(
    state: AppState<A>,
    request: GraphQLRequest,
    _trace_context: Option<fraiseql_core::federation::FederationTraceContext>,
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
            metrics.queries_error.fetch_add(1, Ordering::Relaxed);
            metrics.execution_errors_total.fetch_add(1, Ordering::Relaxed);
            // Record duration even for failed queries
            metrics
                .queries_duration_us
                .fetch_add(elapsed.as_micros() as u64, Ordering::Relaxed);
            ErrorResponse::from_error(GraphQLError::execution(&e.to_string()))
        })?;

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
}
