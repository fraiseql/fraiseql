//! GraphQL HTTP handlers and execution logic.

use std::net::SocketAddr;
use std::sync::atomic::Ordering;
use std::time::Instant;

use axum::{
    Json,
    extract::{Query, State},
    http::HeaderMap,
};
use fraiseql_core::{
    apq::{ApqMetrics, ApqStorage},
    db::traits::DatabaseAdapter,
    security::SecurityContext,
};
use tracing::{debug, error, info, warn};

use super::{
    app_state::AppState,
    request::{GraphQLGetParams, GraphQLRequest, GraphQLResponse},
};
use crate::{
    error::{ErrorResponse, GraphQLError},
    extractors::{OptionalSecurityContext, PeerIp},
    tracing_utils,
};

/// GraphQL HTTP handler for POST requests.
///
/// Handles POST requests to the GraphQL endpoint:
/// 1. Extract W3C trace context from traceparent header (if present)
/// 2. Validate GraphQL request (depth, complexity)
/// 3. Parse GraphQL request body
/// 4. Execute query via Executor with optional `SecurityContext`
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
#[tracing::instrument(skip_all, fields(operation_name))]
pub async fn graphql_handler<A: DatabaseAdapter + Clone + Send + Sync + 'static>(
    State(state): State<AppState<A>>,
    headers: HeaderMap,
    PeerIp(peer_ip): PeerIp,
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

    execute_graphql_request(state, request, trace_context, security_context, &headers, &peer_ip)
        .await
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
/// Returns `413 Payload Too Large` (via `ErrorResponse`) when the query string
/// exceeds `AppState::max_get_query_bytes` (default 100 `KiB`, configurable via
/// `ServerConfig::max_get_query_bytes`). Returns other HTTP status codes for
/// additional error conditions.
///
/// # Note
///
/// Per GraphQL over HTTP spec, GET requests should only be used for queries,
/// not mutations (which should use POST). This handler does not enforce that
/// restriction but logs a warning for mutation-like queries.
#[tracing::instrument(skip_all, fields(operation_name))]
pub async fn graphql_get_handler<A: DatabaseAdapter + Clone + Send + Sync + 'static>(
    State(state): State<AppState<A>>,
    headers: HeaderMap,
    PeerIp(peer_ip): PeerIp,
    OptionalSecurityContext(security_context): OptionalSecurityContext,
    Query(params): Query<GraphQLGetParams>,
) -> Result<GraphQLResponse, ErrorResponse> {
    // Reject oversized GET queries early to prevent DoS via query parsing.
    let max_get_bytes = state.max_get_query_bytes;
    if params.query.len() > max_get_bytes {
        return Err(ErrorResponse::from_error(GraphQLError::request(format!(
            "GET query string exceeds maximum allowed length ({max_get_bytes} bytes)"
        ))));
    }

    // Parse variables from JSON string.
    // Apply the same size cap as the query string — the URL-length limit imposed
    // by reverse proxies/OS is real but not enforced by axum itself, so we guard
    // explicitly to prevent parser DoS from a very large variables value.
    let variables = if let Some(vars_str) = params.variables {
        if vars_str.len() > max_get_bytes {
            return Err(ErrorResponse::from_error(GraphQLError::request(format!(
                "GET variables string exceeds maximum allowed length ({max_get_bytes} bytes)"
            ))));
        }
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

    execute_graphql_request(state, request, trace_context, security_context, &headers, &peer_ip)
        .await
}

/// Extract client IP address from headers.
///
/// # Security
///
/// Does NOT trust X-Forwarded-For or X-Real-IP headers, as these are trivially
/// spoofable by attackers to bypass rate limiting. Returns "unknown" as a safe
/// fallback — callers requiring real IPs should use `ConnectInfo<SocketAddr>`
/// or `ProxyConfig::extract_client_ip()` with validated proxy chains.
#[cfg(feature = "auth")]
#[allow(dead_code)] // Reason: used only in tests that verify spoofable headers are ignored
pub(crate) fn extract_ip_from_headers(_headers: &HeaderMap) -> String {
    // SECURITY: Spoofable headers removed. Use ConnectInfo<SocketAddr> or
    // ProxyConfig::extract_client_ip() for validated IP extraction.
    "unknown".to_string()
}

/// Return the IP address of a TCP peer as a rate-limit key.
///
/// Uses only the IP part of the `SocketAddr` (no port), so all connections
/// from the same client are counted against the same bucket regardless of
/// ephemeral port churn. Returns `"unknown"` when no peer address is available
/// (e.g. in tests that call the handler without `ConnectInfo`).
#[allow(dead_code)] // Reason: used in tests to verify per-IP key logic
pub(crate) fn peer_ip_string(peer: Option<SocketAddr>) -> String {
    peer.map_or_else(|| "unknown".to_string(), |addr: SocketAddr| addr.ip().to_string())
}

/// Extract the APQ SHA-256 hash from the `extensions.persistedQuery` field, if present.
pub(crate) fn extract_apq_hash(extensions: Option<&serde_json::Value>) -> Option<&str> {
    extensions?.get("persistedQuery")?.get("sha256Hash")?.as_str()
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
///
/// # Errors
///
/// Returns [`ErrorResponse`] if the hash doesn't match the body, or if the
/// hash is unknown and no query body was provided (client must retry with full body).
pub(crate) async fn resolve_apq(
    apq_store: &dyn ApqStorage,
    apq_metrics: &ApqMetrics,
    hash: &str,
    query_body: Option<&str>,
) -> Result<String, ErrorResponse> {
    if let Some(body) = query_body {
        // Hash + body present: verify and register.
        if !fraiseql_core::apq::verify_hash(body, hash) {
            apq_metrics.record_error();
            return Err(ErrorResponse::from_error(GraphQLError::persisted_query_mismatch()));
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
            },
            Ok(None) => {
                apq_metrics.record_miss();
                Err(ErrorResponse::from_error(GraphQLError::persisted_query_not_found()))
            },
            Err(e) => {
                warn!(error = %e, "APQ store lookup failed — treating as miss");
                apq_metrics.record_error();
                Err(ErrorResponse::from_error(GraphQLError::persisted_query_not_found()))
            },
        }
    }
}

/// Shared GraphQL execution logic for both GET and POST handlers.
#[tracing::instrument(skip_all, fields(operation_name = request.operation_name.as_deref().unwrap_or("anonymous")))]
async fn execute_graphql_request<A: DatabaseAdapter + Clone + Send + Sync + 'static>(
    state: AppState<A>,
    mut request: GraphQLRequest,
    #[cfg(feature = "federation")] _trace_context: Option<
        fraiseql_core::federation::FederationTraceContext,
    >,
    #[cfg(not(feature = "federation"))] _trace_context: Option<()>,
    mut security_context: Option<SecurityContext>,
    headers: &HeaderMap,
    peer_ip: &str,
) -> Result<GraphQLResponse, ErrorResponse> {
    // API key auth: if configured, try it before falling through to JWT/OIDC.
    if security_context.is_none() {
        if let Some(ref api_key_auth) = state.api_key_authenticator {
            match api_key_auth.authenticate(headers).await {
                crate::api_key::ApiKeyResult::Authenticated(ctx) => {
                    debug!("Authenticated via API key");
                    security_context = Some(*ctx);
                },
                crate::api_key::ApiKeyResult::Invalid => {
                    return Err(ErrorResponse::from_error(GraphQLError::new(
                        "Invalid API key",
                        crate::error::ErrorCode::Unauthenticated,
                    )));
                },
                crate::api_key::ApiKeyResult::NotPresent => {
                    // Fall through to JWT/OIDC (or unauthenticated).
                },
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
            },
            Err(crate::trusted_documents::TrustedDocumentError::ForbiddenRawQuery) => {
                crate::trusted_documents::record_rejected();
                return Err(ErrorResponse::from_error(GraphQLError::forbidden_query()));
            },
            Err(crate::trusted_documents::TrustedDocumentError::DocumentNotFound { id }) => {
                crate::trusted_documents::record_miss();
                return Err(ErrorResponse::from_error(GraphQLError::document_not_found(&id)));
            },
            Err(crate::trusted_documents::TrustedDocumentError::ManifestLoad(msg)) => {
                error!(error = %msg, "Trusted document manifest error");
                return Err(ErrorResponse::from_error(GraphQLError::internal(
                    "Trusted documents unavailable",
                )));
            },
        }
    }

    // Resolve query body — either from APQ or from the request payload.
    let query = if let Some(hash) = extract_apq_hash(request.extensions.as_ref()) {
        if let Some(ref store) = state.apq_store {
            resolve_apq(store.as_ref(), &state.apq_metrics, hash, request.query.as_deref()).await?
        } else {
            // APQ extension present but no store configured — use the body if available.
            request.query.ok_or_else(|| {
                ErrorResponse::from_error(GraphQLError::request(
                    "APQ is not enabled on this server and no query body was provided",
                ))
            })?
        }
    } else {
        request
            .query
            .ok_or_else(|| ErrorResponse::from_error(GraphQLError::request("No query provided")))?
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
            if state.graphql_rate_limiter.check(peer_ip).is_err() {
                return Err(ErrorResponse::from_error(GraphQLError::rate_limited(
                    "Too many validation errors. Please reduce query complexity and try again.",
                )));
            }
        }

        let graphql_error = match e {
            crate::validation::ComplexityValidationError::QueryTooDeep {
                max_depth,
                actual_depth,
            } => GraphQLError::validation(format!(
                "Query exceeds maximum depth: {actual_depth} > {max_depth}"
            )),
            crate::validation::ComplexityValidationError::QueryTooComplex {
                max_complexity,
                actual_complexity,
            } => GraphQLError::validation(format!(
                "Query exceeds maximum complexity: {actual_complexity} > {max_complexity}"
            )),
            crate::validation::ComplexityValidationError::MalformedQuery(msg) => {
                metrics.parse_errors_total.fetch_add(1, Ordering::Relaxed);
                GraphQLError::parse(msg)
            },
            crate::validation::ComplexityValidationError::InvalidVariables(msg) => {
                GraphQLError::request(msg)
            },
            crate::validation::ComplexityValidationError::TooManyAliases {
                max_aliases,
                actual_aliases,
            } => GraphQLError::validation(format!(
                "Query exceeds maximum alias count: {actual_aliases} > {max_aliases}"
            )),
            // Reason: non_exhaustive requires catch-all for cross-crate matches
            _ => GraphQLError::validation("Validation error"),
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
            if state.graphql_rate_limiter.check(peer_ip).is_err() {
                return Err(ErrorResponse::from_error(GraphQLError::rate_limited(
                    "Too many validation errors. Please reduce query complexity and try again.",
                )));
            }
        }

        return Err(ErrorResponse::from_error(GraphQLError::request(e.to_string())));
    }

    // Check federation circuit breaker for _entities queries before execution
    #[cfg(feature = "federation")]
    let cb_entity_types: Vec<String> = if fraiseql_core::federation::is_federation_query(&query) {
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
    #[cfg(not(feature = "federation"))]
    let _cb_entity_types: Vec<String> = vec![];

    // Resolve tenant key from JWT / X-Tenant-ID header / Host header.
    let tenant_key = super::TenantKeyResolver::resolve(
        security_context.as_ref(),
        headers,
        state.domain_registry(),
    )
    .map_err(|e| {
        ErrorResponse::from_error(GraphQLError::new(
            e.to_string(),
            crate::error::ErrorCode::ValidationError,
        ))
    })?;

    // Execute query (defer error propagation to record circuit breaker outcome first)
    let executor = state.executor_for_tenant(tenant_key.as_deref()).map_err(|e| {
        ErrorResponse::from_error(GraphQLError::new(
            e.to_string(),
            crate::error::ErrorCode::Forbidden,
        ))
    })?;
    // Preserve subject for audit logging before security_context is consumed.
    #[cfg(feature = "auth")]
    let audit_subject = security_context.as_ref().map(|ctx| ctx.user_id.clone());
    let exec_result = if let Some(sec_ctx) = security_context {
        executor
            .execute_with_security(&query, request.variables.as_ref(), &sec_ctx)
            .await
    } else {
        executor.execute(&query, request.variables.as_ref()).await
    };

    // Record circuit breaker outcome for federation entity queries
    #[cfg(feature = "federation")]
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
        #[allow(clippy::cast_possible_truncation)]
        // Reason: microsecond counter cannot exceed u64 in any practical uptime
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

        // S46: emit AuthorizationDenied audit event for compliance (SOC 2).
        // Must be emitted before error sanitization so we log the real reason.
        #[cfg(feature = "auth")]
        if matches!(e, fraiseql_core::FraiseQLError::Authorization { .. }) {
            use fraiseql_auth::audit::logger::{AuditEntry, AuditEventType, SecretType, get_audit_logger};
            let resource = if let fraiseql_core::FraiseQLError::Authorization { ref resource, .. } = e {
                resource.clone().unwrap_or_else(|| op_name.to_string())
            } else {
                op_name.to_string()
            };
            get_audit_logger().log_entry(AuditEntry {
                event_type:    AuditEventType::AuthorizationDenied,
                secret_type:   SecretType::JwtToken,
                subject:       audit_subject.clone(),
                operation:     op_name.to_string(),
                success:       false,
                error_message: Some(resource),
                context:       Some(format!("peer_ip={peer_ip}")),
                chain_hash:    None,
            });
        }

        let err = state.error_sanitizer.sanitize(GraphQLError::from_fraiseql_error(&e));
        ErrorResponse::from_error(err)
    })?;

    let elapsed = start_time.elapsed();
    #[allow(clippy::cast_possible_truncation)]
    // Reason: microsecond counter cannot exceed u64 in any practical uptime
    let elapsed_us = elapsed.as_micros() as u64;

    // Record successful query metrics
    metrics.queries_success.fetch_add(1, Ordering::Relaxed);
    metrics.queries_duration_us.fetch_add(elapsed_us, Ordering::Relaxed);
    metrics.db_queries_total.fetch_add(1, Ordering::Relaxed);
    metrics.db_queries_duration_us.fetch_add(elapsed_us, Ordering::Relaxed);
    metrics.operation_metrics.record(op_name, elapsed_us, false);

    // Record federation-specific metrics for federation queries
    #[cfg(feature = "federation")]
    if fraiseql_core::federation::is_federation_query(&query) {
        metrics.record_entity_resolution(elapsed_us, true);
    }

    debug!(
        elapsed_ms = elapsed.as_millis(),
        operation_name = ?request.operation_name,
        "Query executed successfully"
    );

    #[allow(unused_mut)]
    // Reason: mut is required by decrypt_response(&mut ...) when the secrets feature is enabled
    let mut response_json = result;

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
