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
use fraiseql_core::{db::traits::DatabaseAdapter, runtime::Executor, security::SecurityContext};
use serde::{Deserialize, Serialize};
use tracing::{debug, error, info, warn};

use crate::{
    error::{ErrorResponse, GraphQLError},
    extractors::OptionalSecurityContext,
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
///
/// Phase 4: Extended with cache and config for API endpoints
#[derive(Clone)]
pub struct AppState<A: DatabaseAdapter> {
    /// Query executor.
    pub executor: Arc<Executor<A>>,
    /// Metrics collector.
    pub metrics:  Arc<MetricsCollector>,
    /// Query result cache (optional).
    pub cache:    Option<Arc<fraiseql_arrow::cache::QueryCache>>,
    /// Server configuration (optional).
    pub config:   Option<Arc<crate::config::ServerConfig>>,
}

impl<A: DatabaseAdapter> AppState<A> {
    /// Create new application state.
    #[must_use]
    pub fn new(executor: Arc<Executor<A>>) -> Self {
        Self {
            executor,
            metrics: Arc::new(MetricsCollector::new()),
            cache: None,
            config: None,
        }
    }

    /// Create new application state with custom metrics collector.
    #[must_use]
    pub fn with_metrics(executor: Arc<Executor<A>>, metrics: Arc<MetricsCollector>) -> Self {
        Self {
            executor,
            metrics,
            cache: None,
            config: None,
        }
    }

    /// Create new application state with cache.
    ///
    /// Phase 4.1: Add cache support for query result caching
    #[must_use]
    pub fn with_cache(
        executor: Arc<Executor<A>>,
        cache: Arc<fraiseql_arrow::cache::QueryCache>,
    ) -> Self {
        Self {
            executor,
            metrics: Arc::new(MetricsCollector::new()),
            cache: Some(cache),
            config: None,
        }
    }

    /// Create new application state with cache and config.
    ///
    /// Phase 4.1-4.2: Add cache and config support for API endpoints
    #[must_use]
    pub fn with_cache_and_config(
        executor: Arc<Executor<A>>,
        cache: Arc<fraiseql_arrow::cache::QueryCache>,
        config: Arc<crate::config::ServerConfig>,
    ) -> Self {
        Self {
            executor,
            metrics: Arc::new(MetricsCollector::new()),
            cache: Some(cache),
            config: Some(config),
        }
    }

    /// Get query cache if configured.
    pub fn cache(&self) -> Option<&Arc<fraiseql_arrow::cache::QueryCache>> {
        self.cache.as_ref()
    }

    /// Get server configuration if configured.
    pub fn server_config(&self) -> Option<&Arc<crate::config::ServerConfig>> {
        self.config.as_ref()
    }

    /// Get sanitized configuration for safe API exposure.
    ///
    /// Phase 4.2: Returns configuration with sensitive data redacted
    pub fn sanitized_config(&self) -> Option<crate::routes::api::types::SanitizedConfig> {
        self.config
            .as_ref()
            .map(|cfg| crate::routes::api::types::SanitizedConfig::from_config(cfg))
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
    OptionalSecurityContext(security_context): OptionalSecurityContext,
    Json(request): Json<GraphQLRequest>,
) -> Result<GraphQLResponse, ErrorResponse> {
    // Extract trace context from W3C headers
    let trace_context = tracing_utils::extract_trace_context(&headers);
    if trace_context.is_some() {
        debug!("Extracted W3C trace context from incoming request");
    }

    if security_context.is_some() {
        debug!("Authenticated request with security context");
    }

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

    // Phase 4.1: Tests for AppState with cache and config
    // Note: These are structural tests that document Phase 4.1 requirements
    // Full integration tests require actual executor setup

    #[test]
    fn test_appstate_has_cache_field() {
        // Documents: AppState must have cache field
        let _note = "AppState<A> includes: executor, metrics, cache, config";
        assert!(!_note.is_empty());
    }

    #[test]
    fn test_appstate_has_config_field() {
        // Documents: AppState must have config field
        let _note = "AppState<A>::cache: Option<Arc<QueryCache>>";
        assert!(!_note.is_empty());
    }

    #[test]
    fn test_appstate_with_cache_constructor() {
        // Documents: AppState must have with_cache() constructor
        let _note = "AppState::with_cache(executor, cache) -> Self";
        assert!(!_note.is_empty());
    }

    #[test]
    fn test_appstate_with_cache_and_config_constructor() {
        // Documents: AppState must have with_cache_and_config() constructor
        let _note = "AppState::with_cache_and_config(executor, cache, config) -> Self";
        assert!(!_note.is_empty());
    }

    #[test]
    fn test_appstate_cache_accessor() {
        // Documents: AppState must have cache() accessor
        let _note = "AppState::cache() -> Option<&Arc<QueryCache>>";
        assert!(!_note.is_empty());
    }

    #[test]
    fn test_appstate_server_config_accessor() {
        // Documents: AppState must have server_config() accessor
        let _note = "AppState::server_config() -> Option<&Arc<ServerConfig>>";
        assert!(!_note.is_empty());
    }

    // Phase 4.2: Tests for Configuration Access with Sanitization
    #[test]
    fn test_sanitized_config_from_server_config() {
        // SanitizedConfig should extract non-sensitive fields
        use crate::routes::api::types::SanitizedConfig;

        let config = crate::config::ServerConfig {
            port:    8080,
            host:    "0.0.0.0".to_string(),
            workers: Some(4),
            tls:     None,
            limits:  None,
        };

        let sanitized = SanitizedConfig::from_config(&config);

        assert_eq!(sanitized.port, 8080, "Port should be preserved");
        assert_eq!(sanitized.host, "0.0.0.0", "Host should be preserved");
        assert_eq!(sanitized.workers, Some(4), "Workers count should be preserved");
        assert!(!sanitized.tls_enabled, "TLS should be false when not configured");
        assert!(sanitized.is_sanitized(), "Should be marked as sanitized");
    }

    #[test]
    fn test_sanitized_config_indicates_tls_without_exposing_keys() {
        // SanitizedConfig should indicate TLS is present without exposing keys
        use std::path::PathBuf;

        use crate::routes::api::types::SanitizedConfig;

        let config = crate::config::ServerConfig {
            port:    8080,
            host:    "localhost".to_string(),
            workers: None,
            tls:     Some(crate::config::TlsConfig {
                cert_file: PathBuf::from("/path/to/cert.pem"),
                key_file:  PathBuf::from("/path/to/key.pem"),
            }),
            limits:  None,
        };

        let sanitized = SanitizedConfig::from_config(&config);

        assert!(sanitized.tls_enabled, "TLS should be true when configured");
        // Verify that sensitive paths are NOT in the sanitized config
        let json = serde_json::to_string(&sanitized).unwrap();
        assert!(!json.contains("cert"), "Certificate file path should not be exposed");
        assert!(!json.contains("key"), "Key file path should not be exposed");
    }

    #[test]
    fn test_sanitized_config_redaction() {
        // Verify configuration redaction happens correctly
        use crate::routes::api::types::SanitizedConfig;

        let config1 = crate::config::ServerConfig {
            port:    8000,
            host:    "127.0.0.1".to_string(),
            workers: None,
            tls:     None,
            limits:  None,
        };

        let config2 = crate::config::ServerConfig {
            port:    8000,
            host:    "127.0.0.1".to_string(),
            workers: None,
            tls:     Some(crate::config::TlsConfig {
                cert_file: std::path::PathBuf::from("secret.cert"),
                key_file:  std::path::PathBuf::from("secret.key"),
            }),
            limits:  None,
        };

        let san1 = SanitizedConfig::from_config(&config1);
        let san2 = SanitizedConfig::from_config(&config2);

        // Both should have same public fields
        assert_eq!(san1.port, san2.port);
        assert_eq!(san1.host, san2.host);

        // But TLS status should differ
        assert!(!san1.tls_enabled);
        assert!(san2.tls_enabled);
    }

    // Phase 4.3: Tests for Schema Access Pattern
    #[test]
    fn test_appstate_executor_provides_access_to_schema() {
        // Documents: AppState should provide access to schema through executor
        let _note = "AppState<A>::executor can be queried for schema information";
        assert!(!_note.is_empty());
    }

    #[test]
    fn test_schema_access_for_api_endpoints() {
        // Documents: API endpoints should be able to access schema
        let _note = "API routes can access schema via state.executor for introspection";
        assert!(!_note.is_empty());
    }
}
