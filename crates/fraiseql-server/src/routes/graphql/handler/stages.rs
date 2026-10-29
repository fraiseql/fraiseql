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
    graphql::parse_graphql_document,
    security::{IntrospectionEnforcer, SecurityContext, SecurityError},
};
use tracing::{debug, error, warn};

use super::{
    Method, detect_mutation_name, extract_apq_hash, extract_document_id, non_query_operation,
    resolve_apq,
};
use crate::{
    admission::{self, AdmissionPolicy},
    error::{ErrorResponse, GraphQLError},
    routes::graphql::{app_state::AppState, request::GraphQLRequest},
};

/// Who is asking and what they are asking: the request after the stages every
/// `/graphql` entry point runs before it plans anything.
pub(super) struct Admitted {
    /// The principal, authenticated, stamped with the request's trace and enriched.
    pub identified: admission::Identified,
    /// The document the request executes, resolved and allowed for its method.
    pub query:      String,
}

/// Run the admission prefix: [`admission::identify`] under
/// [`AdmissionPolicy::GRAPHQL`], then [`resolve_query_body`].
///
/// The buffered handler and the SSE handler both start here, so a stage added to one
/// reaches the other. They used not to: the SSE handler ran its own copy of the prefix,
/// which lacked enrichment, so a root `@stream` executed for a subject the enrichment
/// gate denies (GHSA-h22c-5xf9-g5vw).
///
/// # Errors
///
/// The first refusal among the stages', unchanged.
pub(super) async fn admit(
    state: &AppState,
    headers: &HeaderMap,
    security_context: Option<SecurityContext>,
    request: &mut GraphQLRequest,
    method: Method,
) -> Result<Admitted, ErrorResponse> {
    // Each stage is boxed for the reason `execute_graphql_request` gives.
    let identified =
        Box::pin(admission::identify(state, headers, security_context, &AdmissionPolicy::GRAPHQL))
            .await
            .map_err(|refusal| ErrorResponse::from_error(super::admission_error(&refusal)))?;
    let query = Box::pin(resolve_query_body(state, request, method)).await?;
    Ok(Admitted { identified, query })
}

/// Determine the GraphQL document this request executes, and refuse one its method
/// may not run.
///
/// Trusted documents take priority over APQ: when a trusted-document store is
/// configured it resolves the document ID first and *replaces* `request.query`,
/// so APQ and execution both see the resolved body rather than whatever the
/// client sent. A raw query against a store that forbids them is rejected here
/// and never reaches the parser.
///
/// # What the method allows
///
/// Judged on the **resolved** document. A persisted query or a trusted document
/// is not in the request, so a check on `request.query` alone let a hash name a
/// mutation and run it over a method that may not write (#1564):
///
/// - `GET` runs no mutation (GraphQL over HTTP; it would sidestep the POST-only CSRF posture), and
///   registers no persisted query: a hash and its body sent over `GET` are verified and executed,
///   never stored — registration is a write, and `POST`'s.
/// - `QUERY` runs queries only (it is safe and idempotent, so an intermediary may replay it), and
///   registers nothing either.
///
/// # Errors
///
/// Returns the trusted-document rejection, the APQ mismatch/not-found error, a
/// request error when no query was supplied at all, or a 405 when the method may
/// not run the document's operation.
pub(super) async fn resolve_query_body(
    state: &AppState,
    request: &mut GraphQLRequest,
    method: Method,
) -> Result<String, ErrorResponse> {
    let query = resolve_document(state, request, method == Method::Post).await?;
    match method {
        Method::Post => {},
        Method::Get => {
            if detect_mutation_name(&query).is_some() {
                warn!(
                    operation_name = ?request.operation_name,
                    "Mutation sent via GET request — rejected (use POST)"
                );
                return Err(ErrorResponse::from_error(GraphQLError::method_not_allowed(
                    "Mutations must be sent over POST, not GET",
                )));
            }
        },
        Method::Query => {
            if let Some(op) = non_query_operation(&query) {
                warn!(operation = op, "Non-query operation sent via QUERY — rejected (use POST)");
                return Err(ErrorResponse::from_error(GraphQLError::method_not_allowed(
                    "Only query operations may be sent over the QUERY method; use POST",
                )));
            }
        },
    }
    Ok(query)
}

/// The document `request` names: its trusted document, its persisted query, or its
/// body. `register` lets a persisted query with its body be stored.
async fn resolve_document(
    state: &AppState,
    request: &mut GraphQLRequest,
    register: bool,
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
            return resolve_apq(
                store.as_ref(),
                &state.apq_metrics,
                hash,
                request.query.as_deref(),
                register,
            )
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
pub(super) fn enforce_introspection_policy(
    state: &AppState,
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
    // The service SDL is not introspection, so it is served whatever the policy is: a
    // gateway composes it from a subgraph whose introspection is off. It answers its own
    // switch instead.
    if state.service_sdl_require_auth
        && security_context.is_none()
        && IntrospectionEnforcer::selects_service_sdl(query)
    {
        debug!("Service SDL query rejected: authentication required");
        state.metrics.queries_error.fetch_add(1, Ordering::Relaxed);
        return Err(ErrorResponse::from_error(GraphQLError::introspection_disabled(
            "The federation service SDL requires authentication",
        )));
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
pub(super) fn validate_request(
    state: &AppState,
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
fn check_validation_error_budget(state: &AppState, peer_ip: &str) -> Result<(), ErrorResponse> {
    if validation_error_budget_spent(state, peer_ip) {
        return Err(ErrorResponse::from_error(GraphQLError::rate_limited(
            "Too many validation errors. Please reduce query complexity and try again.",
        )));
    }
    Ok(())
}

/// Whether the peer has spent its validation-error budget: the GraphQL rate limiter,
/// which only a build with `auth` carries. Without it there is no budget to spend.
#[cfg(feature = "auth")]
fn validation_error_budget_spent(state: &AppState, peer_ip: &str) -> bool {
    state.graphql_rate_limiter.check(peer_ip).is_err()
}

#[cfg(not(feature = "auth"))]
const fn validation_error_budget_spent(_state: &AppState, _peer_ip: &str) -> bool {
    false
}

/// Refuse an `_entities` request for any entity type whose circuit breaker is
/// open, and return the entity types whose outcome the caller must record.
///
/// # Errors
///
/// Returns a `circuit_breaker_open` error carrying the retry hint.
#[cfg(feature = "federation")]
pub(super) fn check_federation_circuit_breakers(
    state: &AppState,
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

/// Whether a response may carry field-level-encrypted values for
/// [`decrypt_response_fields`] to decrypt.
#[cfg(feature = "secrets")]
pub(super) fn decrypts_response(state: &AppState) -> bool {
    state.field_encryption.as_ref().is_some_and(|e| e.has_encrypted_fields())
}

/// Whether a response may carry encrypted values: never, without field encryption built in.
#[cfg(not(feature = "secrets"))]
pub(super) const fn decrypts_response(_state: &AppState) -> bool {
    false
}

/// Decrypt any field-level-encrypted values in the response.
///
/// # Errors
///
/// Returns a sanitized internal error if decryption fails — never the
/// ciphertext, and never a partially decrypted body.
#[cfg(feature = "secrets")]
pub(super) async fn decrypt_response_fields(
    state: &AppState,
    response: &mut serde_json::Value,
) -> Result<(), ErrorResponse> {
    let Some(ref encryption) = state.field_encryption else {
        return Ok(());
    };
    if !decrypts_response(state) {
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
