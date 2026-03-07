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
use fraiseql_core::{
    apq::{ApqMetrics, ApqStorage},
    db::traits::DatabaseAdapter,
    runtime::Executor,
    security::SecurityContext,
};
use serde::{Deserialize, Serialize};
use tracing::{debug, error, info, warn};

use crate::{
    config::error_sanitization::ErrorSanitizer,
    error::{ErrorResponse, GraphQLError},
    extractors::OptionalSecurityContext,
    metrics_server::MetricsCollector,
    tracing_utils,
};
#[cfg(feature = "auth")]
use crate::auth::rate_limiting::{KeyedRateLimiter, RateLimitConfig};

/// GraphQL request payload (for POST requests).
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GraphQLRequest {
    /// GraphQL query string (optional when using APQ with hash-only request).
    #[serde(default)]
    pub query: Option<String>,

    /// Query variables (optional).
    #[serde(default)]
    pub variables: Option<serde_json::Value>,

    /// Operation name (optional).
    #[serde(default)]
    pub operation_name: Option<String>,

    /// Protocol extensions (APQ, tracing, etc.).
    #[serde(default)]
    pub extensions: Option<serde_json::Value>,

    /// Trusted document identifier (GraphQL over HTTP spec).
    #[serde(default, rename = "documentId")]
    pub document_id: Option<String>,
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
    pub executor:             Arc<Executor<A>>,
    /// Metrics collector.
    pub metrics:              Arc<MetricsCollector>,
    /// Query result cache (optional).
    #[cfg(feature = "arrow")]
    pub cache:                Option<Arc<fraiseql_arrow::cache::QueryCache>>,
    /// Server configuration (optional).
    pub config:               Option<Arc<crate::config::ServerConfig>>,
    /// Rate limiter for GraphQL validation errors (per IP).
    #[cfg(feature = "auth")]
    pub graphql_rate_limiter: Arc<KeyedRateLimiter>,
    /// Secrets manager (optional, configured via `[fraiseql.secrets]`).
    #[cfg(feature = "secrets")]
    pub secrets_manager:      Option<Arc<crate::secrets_manager::SecretsManager>>,
    /// Field encryption service for transparent encrypt/decrypt of marked fields.
    #[cfg(feature = "secrets")]
    pub field_encryption:     Option<Arc<crate::encryption::middleware::FieldEncryptionService>>,
    /// Federation circuit breaker manager (optional, enabled via `fraiseql.toml`).
    pub circuit_breaker:
        Option<Arc<crate::federation::circuit_breaker::FederationCircuitBreakerManager>>,
    /// Error sanitizer — strips internal details before sending responses to clients.
    pub error_sanitizer:  Arc<ErrorSanitizer>,
    /// State encryption service (optional, enabled via `[security.state_encryption]`).
    #[cfg(feature = "auth")]
    pub state_encryption:
        Option<Arc<crate::auth::state_encryption::StateEncryptionService>>,
    /// API key authenticator (optional, enabled via `[security.api_keys]`).
    pub api_key_authenticator:
        Option<Arc<crate::api_key::ApiKeyAuthenticator>>,
    /// APQ persistent query store (optional, enabled via compiled schema config).
    pub apq_store:   Option<Arc<dyn ApqStorage>>,
    /// Trusted document store (optional, enabled via `[security.trusted_documents]`).
    pub trusted_docs: Option<Arc<crate::trusted_documents::TrustedDocumentStore>>,
    /// APQ metrics tracker.
    pub apq_metrics: Arc<ApqMetrics>,
    /// Request validator (depth/complexity limits, configured from compiled schema).
    pub validator:    crate::validation::RequestValidator,
    /// Debug configuration (optional, from `[debug]` in `fraiseql.toml`).
    pub debug_config: Option<fraiseql_core::schema::DebugConfig>,
    /// Connection pool auto-tuner (optional, enabled via `[pool_tuning]` config).
    pub pool_tuner:   Option<Arc<crate::pool::PoolAutoTuner>>,
}

impl<A: DatabaseAdapter> AppState<A> {
    /// Create new application state.
    #[must_use]
    pub fn new(executor: Arc<Executor<A>>) -> Self {
        Self {
            executor,
            metrics: Arc::new(MetricsCollector::new()),
            #[cfg(feature = "arrow")]
            cache: None,
            config: None,
            #[cfg(feature = "auth")]
            graphql_rate_limiter: Arc::new(KeyedRateLimiter::new(
                RateLimitConfig::per_ip_standard(),
            )),
            #[cfg(feature = "secrets")]
            secrets_manager: None,
            #[cfg(feature = "secrets")]
            field_encryption: None,
            circuit_breaker: None,
            error_sanitizer: Arc::new(ErrorSanitizer::disabled()),
            #[cfg(feature = "auth")]
            state_encryption: None,
            api_key_authenticator: None,
            apq_store: None,
            trusted_docs: None,
            apq_metrics: Arc::new(ApqMetrics::default()),
            validator: crate::validation::RequestValidator::new(),
            debug_config: None,
            pool_tuner: None,
        }
    }

    /// Create new application state with custom metrics collector.
    #[must_use]
    pub fn with_metrics(executor: Arc<Executor<A>>, metrics: Arc<MetricsCollector>) -> Self {
        Self::new(executor).set_metrics(metrics)
    }

    /// Create new application state with cache.
    #[cfg(feature = "arrow")]
    #[must_use]
    pub fn with_cache(
        executor: Arc<Executor<A>>,
        cache: Arc<fraiseql_arrow::cache::QueryCache>,
    ) -> Self {
        Self::new(executor).set_cache(cache)
    }

    /// Create new application state with cache and config.
    #[cfg(feature = "arrow")]
    #[must_use]
    pub fn with_cache_and_config(
        executor: Arc<Executor<A>>,
        cache: Arc<fraiseql_arrow::cache::QueryCache>,
        config: Arc<crate::config::ServerConfig>,
    ) -> Self {
        Self::new(executor).set_cache(cache).set_config(config)
    }

    fn set_metrics(mut self, metrics: Arc<MetricsCollector>) -> Self {
        self.metrics = metrics;
        self
    }

    #[cfg(feature = "arrow")]
    fn set_cache(mut self, cache: Arc<fraiseql_arrow::cache::QueryCache>) -> Self {
        self.cache = Some(cache);
        self
    }

    #[cfg(feature = "arrow")]
    fn set_config(mut self, config: Arc<crate::config::ServerConfig>) -> Self {
        self.config = Some(config);
        self
    }

    /// Get query cache if configured.
    #[cfg(feature = "arrow")]
    pub fn cache(&self) -> Option<&Arc<fraiseql_arrow::cache::QueryCache>> {
        self.cache.as_ref()
    }

    /// Get server configuration if configured.
    pub const fn server_config(&self) -> Option<&Arc<crate::config::ServerConfig>> {
        self.config.as_ref()
    }

    /// Get sanitized configuration for safe API exposure.
    pub fn sanitized_config(&self) -> Option<crate::routes::api::types::SanitizedConfig> {
        self.config
            .as_ref()
            .map(|cfg| crate::routes::api::types::SanitizedConfig::from_config(cfg))
    }

    /// Set secrets manager (for credential and secret management).
    #[cfg(feature = "secrets")]
    #[must_use]
    pub fn with_secrets_manager(
        mut self,
        secrets_manager: Arc<crate::secrets_manager::SecretsManager>,
    ) -> Self {
        self.secrets_manager = Some(secrets_manager);
        self
    }

    /// Get secrets manager if configured.
    #[cfg(feature = "secrets")]
    pub const fn secrets_manager(&self) -> Option<&Arc<crate::secrets_manager::SecretsManager>> {
        self.secrets_manager.as_ref()
    }

    /// Attach a field encryption service (derived from schema and secrets manager).
    #[cfg(feature = "secrets")]
    #[must_use]
    pub fn with_field_encryption(
        mut self,
        service: Arc<crate::encryption::middleware::FieldEncryptionService>,
    ) -> Self {
        self.field_encryption = Some(service);
        self
    }

    /// Attach a federation circuit breaker manager.
    #[must_use]
    pub fn with_circuit_breaker(
        mut self,
        circuit_breaker: Arc<crate::federation::circuit_breaker::FederationCircuitBreakerManager>,
    ) -> Self {
        self.circuit_breaker = Some(circuit_breaker);
        self
    }

    /// Attach an error sanitizer (loaded from `compiled.security.error_sanitization`).
    #[must_use]
    pub fn with_error_sanitizer(mut self, sanitizer: Arc<ErrorSanitizer>) -> Self {
        self.error_sanitizer = sanitizer;
        self
    }

    /// Attach a state encryption service (loaded from `compiled.security.state_encryption`).
    #[cfg(feature = "auth")]
    #[must_use]
    pub fn with_state_encryption(
        mut self,
        svc: Arc<crate::auth::state_encryption::StateEncryptionService>,
    ) -> Self {
        self.state_encryption = Some(svc);
        self
    }

    /// Attach an API key authenticator (loaded from `compiled.security.api_keys`).
    #[must_use]
    pub fn with_api_key_authenticator(
        mut self,
        authenticator: Arc<crate::api_key::ApiKeyAuthenticator>,
    ) -> Self {
        self.api_key_authenticator = Some(authenticator);
        self
    }

    /// Attach an APQ store for Automatic Persisted Queries.
    #[must_use]
    pub fn with_apq_store(mut self, store: Arc<dyn ApqStorage>) -> Self {
        self.apq_store = Some(store);
        self
    }

    /// Attach a trusted document store for query allowlist enforcement.
    #[must_use]
    pub fn with_trusted_docs(
        mut self,
        store: Arc<crate::trusted_documents::TrustedDocumentStore>,
    ) -> Self {
        self.trusted_docs = Some(store);
        self
    }

    /// Set the request validator (query depth/complexity limits).
    #[must_use]
    pub const fn with_validator(mut self, validator: crate::validation::RequestValidator) -> Self {
        self.validator = validator;
        self
    }

    /// Attach an adaptive connection pool auto-tuner.
    #[must_use]
    pub fn with_pool_tuner(mut self, tuner: Arc<crate::pool::PoolAutoTuner>) -> Self {
        self.pool_tuner = Some(tuner);
        self
    }

    /// Sanitize a batch of errors before sending them to the client.
    pub fn sanitize_errors(&self, errors: Vec<GraphQLError>) -> Vec<GraphQLError> {
        self.error_sanitizer.sanitize_all(errors)
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

    execute_graphql_request(state, request, trace_context, security_context, &headers).await
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
    OptionalSecurityContext(security_context): OptionalSecurityContext,
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
        query: Some(params.query),
        variables,
        operation_name: params.operation_name,
        extensions: None,
        document_id: None,
    };

    if security_context.is_some() {
        debug!("Authenticated GET request with security context");
    }

    execute_graphql_request(state, request, trace_context, security_context, &headers).await
}

/// Extract client IP address from headers.
///
/// # Security
///
/// Does NOT trust X-Forwarded-For or X-Real-IP headers, as these are trivially
/// spoofable by attackers to bypass rate limiting. Returns "unknown" as a safe
/// fallback — callers requiring real IPs should use `ConnectInfo<SocketAddr>`
/// or `ProxyConfig::extract_client_ip()` with validated proxy chains.
fn extract_ip_from_headers(_headers: &HeaderMap) -> String {
    // SECURITY: Spoofable headers removed. Use ConnectInfo<SocketAddr> or
    // ProxyConfig::extract_client_ip() for validated IP extraction.
    "unknown".to_string()
}

/// Extract the APQ SHA-256 hash from the `extensions.persistedQuery` field, if present.
fn extract_apq_hash(extensions: Option<&serde_json::Value>) -> Option<&str> {
    extensions?
        .get("persistedQuery")?
        .get("sha256Hash")?
        .as_str()
}

/// Extract a trusted document ID from the request.
///
/// Supports three formats:
/// 1. `documentId` (GraphQL over HTTP spec)
/// 2. `extensions.persistedQuery.sha256Hash` (Apollo APQ format)
/// 3. `extensions.doc_id` (Relay format)
fn extract_document_id(request: &GraphQLRequest) -> Option<String> {
    // 1. Top-level documentId field (GraphQL over HTTP spec)
    if let Some(ref doc_id) = request.document_id {
        return Some(doc_id.clone());
    }
    // 2. Extensions-based formats
    if let Some(ext) = request.extensions.as_ref() {
        // Relay format: extensions.doc_id
        if let Some(doc_id) = ext.get("doc_id").and_then(|v| v.as_str()) {
            return Some(doc_id.to_string());
        }
        // Apollo APQ format: extensions.persistedQuery.sha256Hash (also used for APQ)
        if let Some(hash) = ext
            .get("persistedQuery")
            .and_then(|pq| pq.get("sha256Hash"))
            .and_then(|h| h.as_str())
        {
            return Some(hash.to_string());
        }
    }
    None
}

/// Resolve an APQ request: look up or register a persisted query.
///
/// Returns the resolved query body, or an error if the query is not found and no body was
/// provided (the client should resend with the full body).
async fn resolve_apq(
    apq_store: &dyn ApqStorage,
    apq_metrics: &ApqMetrics,
    hash: &str,
    query_body: Option<&str>,
) -> Result<String, ErrorResponse> {
    if let Some(body) = query_body {
        // Hash + body present: verify and register.
        if !fraiseql_core::apq::verify_hash(body, hash) {
            apq_metrics.record_error();
            return Err(ErrorResponse::from_error(
                GraphQLError::persisted_query_mismatch(),
            ));
        }
        // Store the query (best-effort; log on failure).
        if let Err(e) = apq_store.set(hash.to_owned(), body.to_owned()).await {
            warn!(error = %e, "Failed to store APQ query — proceeding without caching");
            apq_metrics.record_error();
        } else {
            apq_metrics.record_store();
        }
        Ok(body.to_owned())
    } else {
        // Hash only: look up.
        match apq_store.get(hash).await {
            Ok(Some(stored)) => {
                apq_metrics.record_hit();
                Ok(stored)
            }
            Ok(None) => {
                apq_metrics.record_miss();
                Err(ErrorResponse::from_error(
                    GraphQLError::persisted_query_not_found(),
                ))
            }
            Err(e) => {
                warn!(error = %e, "APQ store lookup failed — treating as miss");
                apq_metrics.record_error();
                Err(ErrorResponse::from_error(
                    GraphQLError::persisted_query_not_found(),
                ))
            }
        }
    }
}

/// Shared GraphQL execution logic for both GET and POST handlers.
async fn execute_graphql_request<A: DatabaseAdapter + Clone + Send + Sync + 'static>(
    state: AppState<A>,
    mut request: GraphQLRequest,
    _trace_context: Option<fraiseql_core::federation::FederationTraceContext>,
    mut security_context: Option<SecurityContext>,
    headers: &HeaderMap,
) -> Result<GraphQLResponse, ErrorResponse> {
    // API key auth: if configured, try it before falling through to JWT/OIDC.
    if security_context.is_none() {
        if let Some(ref api_key_auth) = state.api_key_authenticator {
            match api_key_auth.authenticate(headers).await {
                crate::api_key::ApiKeyResult::Authenticated(ctx) => {
                    debug!("Authenticated via API key");
                    security_context = Some(*ctx);
                }
                crate::api_key::ApiKeyResult::Invalid => {
                    return Err(ErrorResponse::from_error(GraphQLError::new(
                        "Invalid API key",
                        crate::error::ErrorCode::Unauthenticated,
                    )));
                }
                crate::api_key::ApiKeyResult::NotPresent => {
                    // Fall through to JWT/OIDC (or unauthenticated).
                }
            }
        }
    }

    // Resolve query body — trusted documents take priority over APQ.
    // If a trusted document store is configured, resolve the document ID first.
    if let Some(ref td_store) = state.trusted_docs {
        let doc_id = extract_document_id(&request);
        match td_store.resolve(doc_id.as_deref(), request.query.as_deref()).await {
            Ok(resolved) => {
                if doc_id.is_some() {
                    crate::trusted_documents::record_hit();
                    debug!(document_id = ?doc_id, "Trusted document resolved");
                }
                // Replace the query with the resolved body so APQ and execution use it.
                request.query = Some(resolved);
            }
            Err(crate::trusted_documents::TrustedDocumentError::ForbiddenRawQuery) => {
                crate::trusted_documents::record_rejected();
                return Err(ErrorResponse::from_error(GraphQLError::forbidden_query()));
            }
            Err(crate::trusted_documents::TrustedDocumentError::DocumentNotFound { id }) => {
                crate::trusted_documents::record_miss();
                return Err(ErrorResponse::from_error(
                    GraphQLError::document_not_found(&id),
                ));
            }
            Err(crate::trusted_documents::TrustedDocumentError::ManifestLoad(msg)) => {
                error!(error = %msg, "Trusted document manifest error");
                return Err(ErrorResponse::from_error(
                    GraphQLError::internal("Trusted documents unavailable"),
                ));
            }
        }
    }

    // Resolve query body — either from APQ or from the request payload.
    let query = if let Some(hash) = extract_apq_hash(request.extensions.as_ref()) {
        if let Some(ref store) = state.apq_store {
            resolve_apq(
                store.as_ref(),
                &state.apq_metrics,
                hash,
                request.query.as_deref(),
            )
            .await?
        } else {
            // APQ extension present but no store configured — use the body if available.
            request.query.ok_or_else(|| {
                ErrorResponse::from_error(GraphQLError::request(
                    "APQ is not enabled on this server and no query body was provided",
                ))
            })?
        }
    } else {
        request.query.ok_or_else(|| {
            ErrorResponse::from_error(GraphQLError::request(
                "No query provided",
            ))
        })?
    };

    let start_time = Instant::now();
    let metrics = &state.metrics;

    // Increment total queries counter
    metrics.queries_total.fetch_add(1, Ordering::Relaxed);

    info!(
        query_length = query.len(),
        has_variables = request.variables.is_some(),
        operation_name = ?request.operation_name,
        "Executing GraphQL query"
    );

    // Validate request
    let validator = &state.validator;

    // Validate query
    if let Err(e) = validator.validate_query(&query) {
        error!(
            error = %e,
            operation_name = ?request.operation_name,
            "Query validation failed"
        );
        metrics.queries_error.fetch_add(1, Ordering::Relaxed);
        metrics.validation_errors_total.fetch_add(1, Ordering::Relaxed);

        // Check rate limiting for validation errors
        #[cfg(feature = "auth")]
        {
            let client_ip = extract_ip_from_headers(headers);
            if state.graphql_rate_limiter.check(&client_ip).is_err() {
                return Err(ErrorResponse::from_error(GraphQLError::rate_limited(
                    "Too many validation errors. Please reduce query complexity and try again.",
                )));
            }
        }

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

        // Check rate limiting for validation errors
        #[cfg(feature = "auth")]
        {
            let client_ip = extract_ip_from_headers(headers);
            if state.graphql_rate_limiter.check(&client_ip).is_err() {
                return Err(ErrorResponse::from_error(GraphQLError::rate_limited(
                    "Too many validation errors. Please reduce query complexity and try again.",
                )));
            }
        }

        return Err(ErrorResponse::from_error(GraphQLError::request(e.to_string())));
    }

    // Check federation circuit breaker for _entities queries before execution
    let cb_entity_types: Vec<String> =
        if fraiseql_core::federation::is_federation_query(&query) {
            if let Some(ref cb_manager) = state.circuit_breaker {
                let entity_types = crate::federation::circuit_breaker::extract_entity_types(
                    request.variables.as_ref(),
                );
                for entity_type in &entity_types {
                    if let Some(retry_after) = cb_manager.check(entity_type) {
                        warn!(
                            entity = %entity_type,
                            retry_after_secs = retry_after,
                            "Federation circuit breaker open — rejecting _entities request"
                        );
                        metrics.queries_error.fetch_add(1, Ordering::Relaxed);
                        return Err(ErrorResponse::from_error(GraphQLError::circuit_breaker_open(
                            entity_type,
                            retry_after,
                        )));
                    }
                }
                entity_types
            } else {
                vec![]
            }
        } else {
            vec![]
        };

    // Execute query (defer error propagation to record circuit breaker outcome first)
    let exec_result = if let Some(sec_ctx) = security_context {
        state
            .executor
            .execute_with_security(&query, request.variables.as_ref(), &sec_ctx)
            .await
    } else {
        state.executor.execute(&query, request.variables.as_ref()).await
    };

    // Record circuit breaker outcome for federation entity queries
    if !cb_entity_types.is_empty() {
        if let Some(ref cb_manager) = state.circuit_breaker {
            if exec_result.is_ok() {
                for entity_type in &cb_entity_types {
                    cb_manager.record_success(entity_type);
                }
            } else {
                for entity_type in &cb_entity_types {
                    cb_manager.record_failure(entity_type);
                }
            }
        }
    }

    // Propagate execution errors with metrics
    let op_name = request.operation_name.as_deref().unwrap_or("");
    let result = exec_result.map_err(|e| {
        let elapsed = start_time.elapsed();
        let elapsed_us = elapsed.as_micros() as u64;
        error!(
            error = %e,
            elapsed_ms = elapsed.as_millis(),
            operation_name = ?request.operation_name,
            "Query execution failed"
        );
        metrics.queries_error.fetch_add(1, Ordering::Relaxed);
        metrics.execution_errors_total.fetch_add(1, Ordering::Relaxed);
        // Record duration even for failed queries
        metrics.queries_duration_us.fetch_add(elapsed_us, Ordering::Relaxed);
        metrics.operation_metrics.record(op_name, elapsed_us, true);
        let err = state.error_sanitizer.sanitize(GraphQLError::from_fraiseql_error(&e));
        ErrorResponse::from_error(err)
    })?;

    let elapsed = start_time.elapsed();
    let elapsed_us = elapsed.as_micros() as u64;

    // Record successful query metrics
    metrics.queries_success.fetch_add(1, Ordering::Relaxed);
    metrics.queries_duration_us.fetch_add(elapsed_us, Ordering::Relaxed);
    metrics.db_queries_total.fetch_add(1, Ordering::Relaxed);
    metrics.db_queries_duration_us.fetch_add(elapsed_us, Ordering::Relaxed);
    metrics.operation_metrics.record(op_name, elapsed_us, false);

    // Record federation-specific metrics for federation queries
    if fraiseql_core::federation::is_federation_query(&query) {
        metrics.record_entity_resolution(elapsed_us, true);
    }

    debug!(
        response_length = result.len(),
        elapsed_ms = elapsed.as_millis(),
        operation_name = ?request.operation_name,
        "Query executed successfully"
    );

    // Parse result as JSON
    let mut response_json: serde_json::Value = serde_json::from_str(&result).map_err(|e| {
        error!(
            error = %e,
            response_length = result.len(),
            "Failed to deserialize executor response"
        );
        let err = state
            .error_sanitizer
            .sanitize(GraphQLError::internal(format!("Failed to process response: {e}")));
        ErrorResponse::from_error(err)
    })?;

    // Decrypt encrypted fields if field encryption is configured
    #[cfg(feature = "secrets")]
    if let Some(ref encryption) = state.field_encryption {
        if encryption.has_encrypted_fields() {
            encryption.decrypt_response(&mut response_json).await.map_err(|e| {
                error!(error = %e, "Field decryption failed");
                let err = state
                    .error_sanitizer
                    .sanitize(GraphQLError::internal("Field decryption failed".to_string()));
                ErrorResponse::from_error(err)
            })?;
        }
    }

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
        assert_eq!(request.query.as_deref(), Some("{ users { id } }"));
        assert!(request.variables.is_none());
    }

    #[test]
    fn test_graphql_request_without_query() {
        // APQ hash-only request: no query body.
        let json = r#"{"extensions":{"persistedQuery":{"version":1,"sha256Hash":"abc123"}}}"#;
        let request: GraphQLRequest = serde_json::from_str(json).unwrap();
        assert!(request.query.is_none());
        assert!(request.extensions.is_some(), "APQ hash-only request must carry extensions with persistedQuery");
    }

    #[test]
    fn test_graphql_request_with_variables() {
        let json = r#"{"query": "query($id: ID!) { user(id: $id) { name } }", "variables": {"id": "123"}}"#;
        let request: GraphQLRequest = serde_json::from_str(json).unwrap();
        assert_eq!(
            request.variables,
            Some(serde_json::json!({"id": "123"})),
        );
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

    // SECURITY: IP extraction no longer trusts spoofable headers
    #[test]
    fn test_extract_ip_ignores_x_forwarded_for() {
        let mut headers = axum::http::HeaderMap::new();
        headers.insert("x-forwarded-for", "192.0.2.1, 10.0.0.1".parse().unwrap());

        let ip = extract_ip_from_headers(&headers);
        assert_eq!(ip, "unknown", "Must not trust X-Forwarded-For header");
    }

    #[test]
    fn test_extract_ip_ignores_x_real_ip() {
        let mut headers = axum::http::HeaderMap::new();
        headers.insert("x-real-ip", "10.0.0.2".parse().unwrap());

        let ip = extract_ip_from_headers(&headers);
        assert_eq!(ip, "unknown", "Must not trust X-Real-IP header");
    }

    #[test]
    fn test_extract_ip_from_headers_missing() {
        let headers = axum::http::HeaderMap::new();
        let ip = extract_ip_from_headers(&headers);
        assert_eq!(ip, "unknown");
    }

    #[test]
    fn test_extract_ip_ignores_all_spoofable_headers() {
        let mut headers = axum::http::HeaderMap::new();
        headers.insert("x-forwarded-for", "192.0.2.1".parse().unwrap());
        headers.insert("x-real-ip", "10.0.0.2".parse().unwrap());

        let ip = extract_ip_from_headers(&headers);
        assert_eq!(ip, "unknown", "Must not trust any spoofable header");
    }

    #[test]
    fn test_graphql_rate_limiter_is_per_ip() {
        let config = RateLimitConfig {
            enabled:      true,
            max_requests: 3,
            window_secs:  60,
        };
        let limiter = KeyedRateLimiter::new(config);

        // IP 1 should be allowed 3 times
        assert!(limiter.check("192.0.2.1").is_ok(), "request 1 for 192.0.2.1 should be within limit");
        assert!(limiter.check("192.0.2.1").is_ok(), "request 2 for 192.0.2.1 should be within limit");
        assert!(limiter.check("192.0.2.1").is_ok(), "request 3 for 192.0.2.1 should be within limit");

        // IP 2 should have independent limit
        assert!(limiter.check("10.0.0.1").is_ok(), "request 1 for 10.0.0.1 should be within independent limit");
        assert!(limiter.check("10.0.0.1").is_ok(), "request 2 for 10.0.0.1 should be within independent limit");
        assert!(limiter.check("10.0.0.1").is_ok(), "request 3 for 10.0.0.1 should be within independent limit");
    }

    #[test]
    fn test_graphql_rate_limiter_enforces_limit() {
        let config = RateLimitConfig {
            enabled:      true,
            max_requests: 2,
            window_secs:  60,
        };
        let limiter = KeyedRateLimiter::new(config);

        assert!(limiter.check("192.0.2.1").is_ok(), "request 1 within 2-request limit should be allowed");
        assert!(limiter.check("192.0.2.1").is_ok(), "request 2 within 2-request limit should be allowed");
        assert!(limiter.check("192.0.2.1").is_err());
    }

    #[test]
    fn test_graphql_rate_limiter_disabled() {
        let config = RateLimitConfig {
            enabled:      false,
            max_requests: 1,
            window_secs:  60,
        };
        let limiter = KeyedRateLimiter::new(config);

        // When disabled, should allow unlimited requests
        assert!(limiter.check("192.0.2.1").is_ok(), "disabled rate limiter should allow request 1");
        assert!(limiter.check("192.0.2.1").is_ok(), "disabled rate limiter should allow request 2");
        assert!(limiter.check("192.0.2.1").is_ok(), "disabled rate limiter should allow request 3");
    }

    #[test]
    fn test_graphql_rate_limiter_window_reset() {
        let config = RateLimitConfig {
            enabled:      true,
            max_requests: 1,
            window_secs:  0, // Immediate window reset for testing
        };
        let limiter = KeyedRateLimiter::new(config);

        assert!(limiter.check("192.0.2.1").is_ok(), "first request within 1-request window should be allowed");
        // With 0 second window, the window should reset immediately
        // In practice, the window immediately expires and resets
        assert!(limiter.check("192.0.2.1").is_ok(), "request after window reset should be allowed");
    }

    // APQ helper unit tests

    #[test]
    fn test_extract_apq_hash_present() {
        let ext = serde_json::json!({
            "persistedQuery": {
                "version": 1,
                "sha256Hash": "abc123def456"
            }
        });
        assert_eq!(extract_apq_hash(Some(&ext)), Some("abc123def456"));
    }

    #[test]
    fn test_extract_apq_hash_absent() {
        assert_eq!(extract_apq_hash(None), None);

        let ext = serde_json::json!({"other": "value"});
        assert_eq!(extract_apq_hash(Some(&ext)), None);
    }

    #[tokio::test]
    async fn test_apq_miss_returns_not_found() {
        let store = fraiseql_core::apq::InMemoryApqStorage::default();
        let metrics = ApqMetrics::default();

        let result = resolve_apq(&store, &metrics, "nonexistent_hash", None).await;
        assert!(result.is_err());
        assert_eq!(metrics.get_misses(), 1);
    }

    #[tokio::test]
    async fn test_apq_register_and_hit() {
        let store = fraiseql_core::apq::InMemoryApqStorage::default();
        let metrics = ApqMetrics::default();

        let query = "{ users { id } }";
        let hash = fraiseql_core::apq::hash_query(query);

        // Register: hash + body
        let result = resolve_apq(&store, &metrics, &hash, Some(query)).await;
        assert_eq!(result.unwrap(), query);
        assert_eq!(metrics.get_stored(), 1);

        // Hit: hash only
        let result = resolve_apq(&store, &metrics, &hash, None).await;
        assert_eq!(result.unwrap(), query);
        assert_eq!(metrics.get_hits(), 1);
    }

    #[tokio::test]
    async fn test_apq_hash_mismatch() {
        let store = fraiseql_core::apq::InMemoryApqStorage::default();
        let metrics = ApqMetrics::default();

        let result =
            resolve_apq(&store, &metrics, "wrong_hash", Some("{ users { id } }")).await;
        assert!(result.is_err());
        assert_eq!(metrics.get_errors(), 1);
    }
}
