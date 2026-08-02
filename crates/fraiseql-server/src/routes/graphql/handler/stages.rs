//! The stages of a `/graphql` request, one function each.
//!
//! `execute_graphql_request` used to be ~580 lines carrying eight sequential
//! concerns inline, several of which are load-bearing security decisions —
//! ambiguous-credential rejection, the introspection policy, the identity
//! enrichment gate, the per-tenant quotas. A regression there is invisible in a
//! diff of a 580-line function, and the decisions had no name to test against
//! (#732).
//!
//! Each stage below is named after what it decides, takes what it needs and
//! nothing more, and returns either its result or the `ErrorResponse` the caller
//! must return. The handler is the sequence of their names. The issue-number
//! annotations stay attached to the stage that owns them.
//!
//! Two things deliberately did **not** move: query execution itself (it must own
//! the tenant-dispatch permit for the rest of the request) and the metrics
//! bookkeeping around it (it reads the timer the caller started).

use std::sync::atomic::Ordering;

use axum::http::HeaderMap;
use fraiseql_core::{
    db::traits::DatabaseAdapter,
    graphql::parse_graphql_document,
    security::{IntrospectionEnforcer, SecurityContext, SecurityError},
};
#[cfg(feature = "federation")]
use tracing::warn;
use tracing::{debug, error};

use super::{extract_apq_hash, extract_document_id, resolve_apq};
use crate::{
    error::{ErrorResponse, GraphQLError},
    routes::graphql::{app_state::AppState, request::GraphQLRequest},
    tracing_utils,
};

/// Resolve the request's principal from the credentials it presents.
///
/// Order is load-bearing: a service account's `run_as` ceiling takes precedence
/// over the scopes-only static API key on the same header (ADR-0018). A JWT
/// principal presented *alongside* a secret header is rejected as ambiguous
/// (#602) rather than silently resolved to one of the two identities.
///
/// `security_context` carries whatever the JWT/OIDC extractor already resolved.
///
/// # Errors
///
/// Returns a 401 response for ambiguous credentials or an invalid API key.
pub(super) async fn authenticate<A: DatabaseAdapter + Clone + Send + Sync + 'static>(
    state: &AppState<A>,
    headers: &HeaderMap,
    mut security_context: Option<SecurityContext>,
) -> Result<Option<SecurityContext>, ErrorResponse> {
    if let Some(ref sa_auth) = state.service_account_authenticator {
        match sa_auth.resolve(headers, security_context.is_some()) {
            crate::service_account::SaAuth::Authenticated(ctx) => {
                debug!("Authenticated via service account");
                security_context = Some(*ctx);
            },
            crate::service_account::SaAuth::Ambiguous => {
                return Err(ErrorResponse::from_error(GraphQLError::new(
                    "Ambiguous credentials: a bearer token and an API-key / service-account \
                     secret were presented on the same request",
                    crate::error::ErrorCode::Unauthenticated,
                )));
            },
            // No secret header, or present-but-unmatched — fall through to the static
            // API-key check (which 401s if it also fails to match).
            crate::service_account::SaAuth::NoSecret
            | crate::service_account::SaAuth::Unmatched => {},
        }
    }

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

    Ok(security_context)
}

/// Stamp the originating request's W3C trace context onto the principal.
///
/// The change-log write records both the scalar `trace_id` and the full parsed
/// `trace_context` JSONB (#375), which is what makes a row traceable back to the
/// request that produced it. Both are gated on a principal being present: an
/// anonymous request has none to stamp, so its change-log rows keep `trace_id`
/// NULL — consistent with how `tenant_id` behaves there.
#[must_use]
pub(super) fn stamp_trace_context(
    headers: &HeaderMap,
    security_context: Option<SecurityContext>,
) -> Option<SecurityContext> {
    let mut security_context = security_context;
    if let Some(trace_id) = tracing_utils::extract_trace_id(headers) {
        security_context = security_context.map(|ctx| ctx.with_trace_id(trace_id));
    }
    if let Some(trace_context) = tracing_utils::extract_trace_context_json(headers) {
        security_context = security_context.map(|ctx| ctx.with_trace_context(trace_context));
    }
    security_context
}

/// Resolve the subject's database identity and merge it under the forge-proof
/// `fraiseql.enriched.*` namespace (#539).
///
/// Runs *before* dispatch so RLS, views and inject-params scope on a DB-derived
/// identity rather than a client-asserted one. Fail-closed at source: an
/// unresolved identity denies the request before any data query runs; a
/// transient resolver failure is a 503. An unauthenticated request has no
/// subject to resolve and proceeds unchanged.
///
/// # Errors
///
/// Returns 403 when the identity is denied and 503 when resolution is
/// unavailable.
#[cfg(feature = "auth")]
pub(super) async fn enrich_identity<A: DatabaseAdapter + Clone + Send + Sync + 'static>(
    state: &AppState<A>,
    security_context: &mut Option<SecurityContext>,
) -> Result<(), ErrorResponse> {
    let Some(resolver) = state.identity_resolver.as_ref() else {
        return Ok(());
    };
    let Some(ctx) = security_context.as_mut() else {
        return Ok(());
    };
    match crate::identity::enrich_security_context(resolver, ctx).await {
        crate::identity::EnrichmentOutcome::Proceed => Ok(()),
        // Generic outward body (DESIGN §5.4): the precise DenyReason is logged
        // server-side, never surfaced (actor-table oracle guard).
        crate::identity::EnrichmentOutcome::Denied => Err(ErrorResponse::from_error(
            GraphQLError::new("Access denied", crate::error::ErrorCode::Forbidden),
        )),
        crate::identity::EnrichmentOutcome::Unavailable => {
            Err(ErrorResponse::from_error(GraphQLError::new(
                "Identity resolution temporarily unavailable",
                crate::error::ErrorCode::ServiceUnavailable,
            )))
        },
    }
}

/// Determine the GraphQL document this request executes.
///
/// Trusted documents take priority over APQ: when a trusted-document store is
/// configured it resolves the document ID first and *replaces* `request.query`,
/// so APQ and execution both see the resolved body rather than whatever the
/// client sent. A raw query against a store that forbids them is rejected here
/// and never reaches the parser.
///
/// # Errors
///
/// Returns the trusted-document rejection, the APQ mismatch/not-found error, or
/// a request error when no query was supplied at all.
pub(super) async fn resolve_query_body<A: DatabaseAdapter + Clone + Send + Sync + 'static>(
    state: &AppState<A>,
    request: &mut GraphQLRequest,
) -> Result<String, ErrorResponse> {
    if let Some(ref td_store) = state.trusted_docs {
        let doc_id = extract_document_id(request);
        match td_store.resolve(doc_id.as_deref(), request.query.as_deref()) {
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

    if let Some(hash) = extract_apq_hash(request.extensions.as_ref()) {
        if let Some(ref store) = state.apq_store {
            return resolve_apq(store.as_ref(), &state.apq_metrics, hash, request.query.as_deref())
                .await;
        }
        // APQ extension present but no store configured — use the body if available.
        return request.query.clone().ok_or_else(|| {
            ErrorResponse::from_error(GraphQLError::request(
                "APQ is not enabled on this server and no query body was provided",
            ))
        });
    }

    request
        .query
        .clone()
        .ok_or_else(|| ErrorResponse::from_error(GraphQLError::request("No query provided")))
}

/// Apply the introspection policy (#453) to the resolved query.
///
/// This is the single choke point every path — POST, GET, APQ, trusted documents
/// — funnels through, so `{ __schema }` / `{ __type }` (single-root, aliased, or
/// multi-root) cannot be served when the policy forbids it. `__typename` and
/// normal queries are never blocked.
///
/// # Errors
///
/// Returns a GraphQL error in `errors[]` with HTTP 200, never a 5xx.
pub(super) fn enforce_introspection_policy<A: DatabaseAdapter + Clone + Send + Sync + 'static>(
    state: &AppState<A>,
    query: &str,
    security_context: Option<&SecurityContext>,
) -> Result<(), ErrorResponse> {
    let enforcer = IntrospectionEnforcer::new(state.introspection_policy);
    let user_id = security_context.map(|ctx| ctx.user_id.as_str());
    if let Err(err) = enforcer.validate_query(query, user_id) {
        debug!(policy = %state.introspection_policy, "Introspection query rejected by policy");
        state.metrics.queries_error.fetch_add(1, Ordering::Relaxed);
        let detail = match err {
            SecurityError::IntrospectionDisabled { detail } => detail,
            other => other.to_string(),
        };
        return Err(ErrorResponse::from_error(GraphQLError::introspection_disabled(detail)));
    }
    Ok(())
}

/// Enforce the structural limits: query depth, complexity, alias count, and the
/// variables payload.
///
/// The document is parsed exactly once here so the validator can walk the AST
/// without re-parsing — and only when the validator might use it, since a no-op
/// validator would make the parse pure waste (the executor parses what it needs
/// through its own cache; see F001 in `docs/history/IMPROVEMENTS.md`).
///
/// Repeated validation failures from one peer are rate-limited: a client probing
/// for the depth ceiling is cheap to answer and expensive to parse for.
///
/// # Errors
///
/// Returns the corresponding validation error, or a 429 when the peer has
/// exceeded its validation-error budget.
pub(super) fn validate_request<A: DatabaseAdapter + Clone + Send + Sync + 'static>(
    state: &AppState<A>,
    query: &str,
    request: &GraphQLRequest,
    peer_ip: &str,
) -> Result<(), ErrorResponse> {
    let validator = &state.validator;
    let metrics = &state.metrics;

    let validation_outcome = if validator.is_no_op() {
        Ok(())
    } else {
        match parse_graphql_document(query) {
            Ok(doc) => validator.validate_query_doc(&doc, request.variables.as_ref()),
            Err(e) => Err(e),
        }
    };
    if let Err(e) = validation_outcome {
        error!(
            error = %e,
            operation_name = ?request.operation_name,
            "Query validation failed"
        );
        metrics.queries_error.fetch_add(1, Ordering::Relaxed);
        metrics.validation_errors_total.fetch_add(1, Ordering::Relaxed);
        check_validation_error_budget(state, peer_ip)?;

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

    if let Err(e) = validator.validate_variables(request.variables.as_ref()) {
        error!(
            error = %e,
            operation_name = ?request.operation_name,
            "Variables validation failed"
        );
        metrics.queries_error.fetch_add(1, Ordering::Relaxed);
        metrics.validation_errors_total.fetch_add(1, Ordering::Relaxed);
        check_validation_error_budget(state, peer_ip)?;

        return Err(ErrorResponse::from_error(GraphQLError::request(e.to_string())));
    }

    Ok(())
}

/// Charge one validation failure against the peer's budget.
///
/// Extracted so both validation branches spend from the same counter — they were
/// two copies of the same block, which is one edit away from only one of them
/// counting.
fn check_validation_error_budget<A: DatabaseAdapter + Clone + Send + Sync + 'static>(
    state: &AppState<A>,
    peer_ip: &str,
) -> Result<(), ErrorResponse> {
    #[cfg(feature = "auth")]
    if state.graphql_rate_limiter.check(peer_ip).is_err() {
        return Err(ErrorResponse::from_error(GraphQLError::rate_limited(
            "Too many validation errors. Please reduce query complexity and try again.",
        )));
    }
    #[cfg(not(feature = "auth"))]
    let _ = (state, peer_ip);
    Ok(())
}

/// Refuse an `_entities` request for any entity type whose circuit breaker is
/// open, and return the entity types whose outcome the caller must record.
///
/// # Errors
///
/// Returns a `circuit_breaker_open` error carrying the retry hint.
#[cfg(feature = "federation")]
pub(super) fn check_federation_circuit_breakers<
    A: DatabaseAdapter + Clone + Send + Sync + 'static,
>(
    state: &AppState<A>,
    query: &str,
    variables: Option<&serde_json::Value>,
) -> Result<Vec<String>, ErrorResponse> {
    if !fraiseql_core::federation::is_federation_query(query) {
        return Ok(vec![]);
    }
    let Some(ref cb_manager) = state.circuit_breaker else {
        return Ok(vec![]);
    };
    let entity_types = crate::federation::circuit_breaker::extract_entity_types(variables);
    for entity_type in &entity_types {
        if let Some(retry_after) = cb_manager.check(entity_type) {
            warn!(
                entity = %entity_type,
                retry_after_secs = retry_after,
                "Federation circuit breaker open — rejecting _entities request"
            );
            state.metrics.queries_error.fetch_add(1, Ordering::Relaxed);
            return Err(ErrorResponse::from_error(GraphQLError::circuit_breaker_open(
                entity_type,
                retry_after,
            )));
        }
    }
    Ok(entity_types)
}

/// Run the `before:mutation` hook chain and return the variables execution
/// should use.
///
/// The registry lookup is a single `HashMap::get`, so this is zero overhead when
/// no hooks are registered. A chain that aborts refuses the request; a chain that
/// fails is an internal error — neither silently proceeds with the original
/// input, which would run a mutation the policy chain declined to approve.
///
/// # Errors
///
/// Returns the abort message as a validation error, or a sanitized internal
/// error when the chain itself fails.
pub(super) async fn run_before_mutation_hooks<
    A: DatabaseAdapter + Clone + Send + Sync + 'static,
>(
    state: &AppState<A>,
    query: &str,
    variables: Option<serde_json::Value>,
) -> Result<Option<serde_json::Value>, ErrorResponse> {
    let Some(ref hooks) = state.before_mutation_hooks else {
        return Ok(variables);
    };
    let Some(mutation_name) = super::detect_mutation_name(query) else {
        return Ok(variables);
    };
    let Some(chain) = hooks.trigger_registry.before_chain(&mutation_name) else {
        return Ok(variables);
    };

    let input = variables.clone().unwrap_or(serde_json::Value::Null);
    let host = fraiseql_functions::NoopHostContext::new(fraiseql_functions::EventPayload {
        trigger_type: format!("before:mutation:{mutation_name}"),
        entity:       mutation_name.clone(),
        event_kind:   "before".to_string(),
        data:         input.clone(),
        timestamp:    chrono::Utc::now(),
    });
    match chain
        .execute(
            input,
            &hooks.module_registry,
            &hooks.observer,
            &host,
            fraiseql_functions::ResourceLimits::default(),
        )
        .await
    {
        Ok(fraiseql_functions::BeforeMutationResult::Proceed(modified)) => {
            Ok((!modified.is_null()).then_some(modified))
        },
        Ok(fraiseql_functions::BeforeMutationResult::Abort(msg)) => {
            Err(ErrorResponse::from_error(GraphQLError::validation(msg)))
        },
        Err(e) => {
            error!(error = %e, mutation = %mutation_name, "before:mutation chain failed");
            Err(ErrorResponse::from_error(
                state
                    .error_sanitizer
                    .sanitize(GraphQLError::internal("before:mutation hook execution failed")),
            ))
        },
        // Reason: BeforeMutationResult is non_exhaustive; treat unknown variants as Proceed
        Ok(_) => Ok(variables),
    }
}

/// Decrypt any field-level-encrypted values in the response.
///
/// # Errors
///
/// Returns a sanitized internal error if decryption fails — never the
/// ciphertext, and never a partially decrypted body.
#[cfg(feature = "secrets")]
pub(super) async fn decrypt_response_fields<A: DatabaseAdapter + Clone + Send + Sync + 'static>(
    state: &AppState<A>,
    response: &mut serde_json::Value,
) -> Result<(), ErrorResponse> {
    let Some(ref encryption) = state.field_encryption else {
        return Ok(());
    };
    if !encryption.has_encrypted_fields() {
        return Ok(());
    }
    encryption.decrypt_response(response).await.map_err(|e| {
        error!(error = %e, "Field decryption failed");
        ErrorResponse::from_error(
            state
                .error_sanitizer
                .sanitize(GraphQLError::internal("Field decryption failed")),
        )
    })
}
