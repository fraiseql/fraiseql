//! GraphQL HTTP handlers and execution logic.

use std::{sync::atomic::Ordering, time::Instant};

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
use fraiseql_error::FraiseQLError;
use tracing::{debug, error, warn};

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
#[doc(hidden)] // Internal-pub: axum route handler wired via Server::route; downstream uses Server::serve(), not this fn directly.
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
#[doc(hidden)] // Internal-pub: axum route handler wired via Server::route; downstream uses Server::serve(), not this fn directly.
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
        return Err(ErrorResponse::from_error(GraphQLError::payload_too_large(format!(
            "GET query string exceeds maximum allowed length ({max_get_bytes} bytes)"
        ))));
    }

    // Parse variables from JSON string.
    // Apply the same size cap as the query string — the URL-length limit imposed
    // by reverse proxies/OS is real but not enforced by axum itself, so we guard
    // explicitly to prevent parser DoS from a very large variables value.
    let variables = if let Some(vars_str) = params.variables {
        if vars_str.len() > max_get_bytes {
            return Err(ErrorResponse::from_error(GraphQLError::payload_too_large(format!(
                "GET variables string exceeds maximum allowed length ({max_get_bytes} bytes)"
            ))));
        }
        match serde_json::from_str::<serde_json::Value>(&vars_str) {
            Ok(v) => Some(v),
            Err(e) => {
                // Log the fault, never the payload. `variables` is client-supplied
                // and may hold PII or bearer tokens; emitting up to
                // `max_get_query_bytes` of it at `warn!` put that into every log
                // sink the deployment ships to (#730). The serde error already
                // carries the line/column, which is what a diagnosis needs.
                warn!(
                    error = %e,
                    variables_bytes = vars_str.len(),
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

    // Reject mutations over GET with 405 per the GraphQL-over-HTTP spec: GET is for
    // queries only, and allowing mutations sidesteps the POST-only CSRF posture
    // (M-get-mutations). Detection parses the operation (reliable) rather than matching a
    // `mutation` string prefix (which a leading comment or named query defeats).
    if detect_mutation_name(&params.query).is_some() {
        warn!(
            operation_name = ?params.operation_name,
            "Mutation sent via GET request — rejected (use POST)"
        );
        return Err(ErrorResponse::from_error(GraphQLError::method_not_allowed(
            "Mutations must be sent over POST, not GET",
        )));
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

/// Extract the mutation name from a GraphQL query string, if the operation is a mutation.
///
/// Returns `Some(root_field_name)` when the query parses successfully and the operation
/// type is `"mutation"`. Returns `None` for queries, subscriptions, or parse errors.
///
/// Used to look up before-mutation hooks: a single `HashMap::get` on the trigger
/// registry — O(1) and allocation-free when no hooks are registered.
pub(crate) fn detect_mutation_name(query: &str) -> Option<String> {
    let parsed = fraiseql_core::graphql::parse_query(query).ok()?;
    if parsed.operation_type == "mutation" {
        Some(parsed.root_field)
    } else {
        None
    }
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
    security_context: Option<SecurityContext>,
    headers: &HeaderMap,
    peer_ip: &str,
) -> Result<GraphQLResponse, ErrorResponse> {
    // ── Who is asking ────────────────────────────────────────────────────────
    //
    // Every `await` on a stage is `Box::pin`ned. An awaited `async fn` embeds its
    // whole state machine in the caller's future, so the nine stages compose into
    // a type deep enough that rustc 1.97 refuses to compute the handler's layout
    // ("queries overflow the depth limit"). Boxing also keeps the request future
    // small enough for `clippy::large_futures`, which this crate has tripped
    // repeatedly on `Server`-shaped futures. The cost is one allocation per stage
    // per request, against a database round-trip.
    let mut security_context =
        Box::pin(stages::authenticate(&state, headers, security_context)).await?;
    security_context = stages::stamp_trace_context(headers, security_context);
    #[cfg(feature = "auth")]
    Box::pin(stages::enrich_identity(&state, &mut security_context)).await?;

    // ── What they are asking ─────────────────────────────────────────────────
    let query = Box::pin(stages::resolve_query_body(&state, &mut request)).await?;

    let start_time = Instant::now();
    let metrics = &state.metrics;
    metrics.queries_total.fetch_add(1, Ordering::Relaxed);

    // Reason (F041): per-request execution log moved to `debug!`. At >100 RPS
    // this event drowns the operator's `info!`-level signal-to-noise ratio.
    // `info!` is reserved for startup/shutdown/schema-reload events.
    debug!(
        query_length = query.len(),
        has_variables = request.variables.is_some(),
        operation_name = ?request.operation_name,
        "Executing GraphQL query"
    );

    // ── Whether they may ─────────────────────────────────────────────────────
    stages::enforce_introspection_policy(&state, &query, security_context.as_ref())?;
    stages::validate_request(&state, &query, &request, peer_ip)?;

    #[cfg(feature = "federation")]
    let cb_entity_types =
        stages::check_federation_circuit_breakers(&state, &query, request.variables.as_ref())?;

    // Resolve tenant key from JWT / X-Tenant-ID header / Host header, through the
    // same seam the MCP transport uses (#858).
    let tenant_key =
        super::tenant_dispatch::resolve_tenant_key(&state, security_context.as_ref(), headers)
            .map_err(|e| {
                ErrorResponse::from_error(GraphQLError::new(
                    e.to_string(),
                    crate::error::ErrorCode::ValidationError,
                ))
            })?;

    // ── Idempotency (#747) ───────────────────────────────────────────────────
    // A mutation carrying an `Idempotency-Key` header executes at most once per
    // key: a repeat with the same body replays the stored response, a repeat
    // with a different body is a 409 conflict. This is the receiving half of
    // the saga at-least-once dispatch contract — a peer coordinator re-sends a
    // step's mutation under the same key after an ambiguous failure (timeout,
    // connection reset after send) or a crash-recovery replay, and this check
    // is what turns those re-sends into one logical effect. Scoped by tenant so
    // keys can never replay across tenants; mutations only arrive via POST (the
    // GET handler rejects them).
    let idempotency_key = if detect_mutation_name(&query).is_some() {
        headers.get("idempotency-key").and_then(|v| v.to_str().ok()).map(|client_key| {
            let scope = crate::routes::idempotency::IdempotencyScope {
                tenant: tenant_key.clone(),
                method: "POST".to_string(),
                path:   "/graphql".to_string(),
            };
            let body_hash = crate::routes::idempotency::hash_body(&serde_json::json!({
                "query": query,
                "variables": request.variables,
                "operationName": request.operation_name,
            }));
            (scope.key(client_key), body_hash)
        })
    } else {
        None
    };
    if let Some((ref key, body_hash)) = idempotency_key {
        match state.idempotency_store.check(key, body_hash).await {
            crate::routes::idempotency::IdempotencyCheck::Replay(stored) => {
                debug!("Replaying stored response for repeated Idempotency-Key mutation");
                return Ok(GraphQLResponse {
                    body: stored.body.unwrap_or(serde_json::Value::Null),
                });
            },
            crate::routes::idempotency::IdempotencyCheck::Conflict => {
                return Err(ErrorResponse::from_error(GraphQLError::idempotency_conflict()));
            },
            crate::routes::idempotency::IdempotencyCheck::New => {},
        }
    }

    let variables =
        Box::pin(stages::run_before_mutation_hooks(&state, &query, request.variables)).await?;

    // ── Execution ────────────────────────────────────────────────────────────
    // Dispatch, the suspended-tenant gate and the per-tenant quotas all live in
    // the shared seam so the MCP transport enforces the identical policy (#858).
    // `dispatch` holds the concurrency permit for the rest of this scope, which
    // is why it is not extracted into a stage.
    let dispatch = super::tenant_dispatch::dispatch_to_tenant(&state, tenant_key.as_deref())
        .map_err(|e| ErrorResponse::from_error(tenant_dispatch_error(&e)))?;
    let executor = &dispatch.executor;

    // M-quotas (cost): reject a query whose estimated cost exceeds the tenant's
    // per-operation cost budget (#379). Same chokepoint and 429 surfacing as the
    // other per-tenant quotas, and in the shared seam for the same reason (#858).
    super::tenant_dispatch::charge_cost_budget(
        &state,
        tenant_key.as_deref(),
        &query,
        variables.as_ref(),
        executor,
    )
    .map_err(|e| ErrorResponse::from_error(tenant_dispatch_error(&e)))?;

    // Preserve subject for audit logging before security_context is consumed.
    #[cfg(feature = "auth")]
    let audit_subject = security_context.as_ref().map(|ctx| ctx.user_id.to_string());
    // Preserve the caller for after:mutation dispatch (#803): the dispatched
    // host's `auth_context` reflects the caller whose request triggered it.
    #[cfg(feature = "functions-runtime")]
    let dispatch_caller = security_context.clone();
    // Error propagation is deferred so the circuit-breaker outcome is recorded first.
    let exec_result = if let Some(sec_ctx) = security_context {
        executor.execute_with_security(&query, variables.as_ref(), &sec_ctx).await
    } else {
        executor.execute(&query, variables.as_ref()).await
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
            use fraiseql_auth::audit::logger::{
                AuditEntry, AuditEventType, SecretType, get_audit_logger,
            };
            let resource =
                if let fraiseql_core::FraiseQLError::Authorization { ref resource, .. } = e {
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

    // ── Post-processing ──────────────────────────────────────────────────────
    #[allow(unused_mut)]
    // Reason: mut is required by decrypt_response_fields(&mut ...) under the secrets feature
    let mut response_json = result;

    #[cfg(feature = "secrets")]
    Box::pin(stages::decrypt_response_fields(&state, &mut response_json)).await?;

    // After-mutation function triggers (#460): once the mutation has committed,
    // fire-and-forget any matching `after:mutation` functions on a live,
    // I/O-capable host context. Gated on `functions-runtime` (the WASM runtime +
    // live host are opt-in); a single `HashMap::get` of zero overhead when no
    // hooks are registered. Errors are logged inside the spawned tasks and never
    // affect the response that was already produced above.
    #[cfg(feature = "functions-runtime")]
    if let Some(ref hooks) = state.before_mutation_hooks {
        if let Some(mutation_name) = detect_mutation_name(&query) {
            let plans = crate::routes::after_mutation::plan_after_mutation_dispatch(
                hooks,
                executor.schema(),
                &mutation_name,
                &response_json,
            );
            if !plans.is_empty() {
                // #594: give each dispatched function the `fraiseql_query` bridge
                // over the request-path executor, run under its own `run_as` ceiling.
                let query_executor_factory =
                    crate::routes::after_mutation::make_query_executor_factory(
                        state.executor.clone(),
                    );
                crate::routes::after_mutation::spawn_after_mutation(
                    hooks,
                    plans,
                    Some(query_executor_factory),
                    dispatch_caller,
                );
            }
        }
    }

    // Idempotency (#747): persist the successful response so a re-send of the
    // same mutation under the same key replays it instead of executing again.
    // Only success is stored — a failed mutation stays retryable under its key.
    if let Some((key, body_hash)) = idempotency_key {
        state
            .idempotency_store
            .store(
                key,
                body_hash,
                crate::routes::idempotency::StoredResponse {
                    status:  200,
                    headers: Vec::new(),
                    body:    Some(response_json.clone()),
                },
            )
            .await;
    }

    Ok(GraphQLResponse {
        body: response_json,
    })
}

/// Map a tenant-dispatch error from [`AppState::executor_for_tenant`] to the
/// correct GraphQL error code (#332).
///
/// `executor_for_tenant` returns [`FraiseQLError::Authorization`] for an unknown
/// tenant key (→ 403 Forbidden) and [`FraiseQLError::ServiceUnavailable`] for a
/// suspended tenant (→ 503 with a `Retry-After` header carrying `retry_after`).
/// Previously both collapsed to 403, discarding the variant and the retry hint.
fn tenant_dispatch_error(error: &FraiseQLError) -> GraphQLError {
    match error {
        FraiseQLError::ServiceUnavailable { retry_after, .. } => {
            GraphQLError::service_unavailable(error.to_string(), *retry_after)
        },
        // Per-tenant concurrency limit reached (M-quotas) → 429 Too Many Requests.
        FraiseQLError::RateLimited { .. } => GraphQLError::rate_limited(error.to_string()),
        // Unknown tenant key (Authorization) and any other dispatch error stay
        // 403 Forbidden, preserving the prior behaviour.
        _ => GraphQLError::new(error.to_string(), crate::error::ErrorCode::Forbidden),
    }
}

mod stages;

#[cfg(test)]
mod tests;
