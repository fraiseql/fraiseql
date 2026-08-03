//! GraphQL-over-SSE response transport and root-field `@stream` delivery (#387).
//!
//! Mounted as an `Accept: text/event-stream` branch **inside** the existing
//! `/graphql` handlers, so it sits behind the same auth `route_layer`, the same
//! content-type enforcement and the same global middleware as the buffered
//! transport — a transport mounted outside that stack is #812.
//!
//! Two modes:
//!
//! - **Single result** (no root `@stream`): the request runs through the ordinary buffered pipeline
//!   once and the response is delivered as one `next` event followed by `complete`
//!   (GraphQL-over-SSE "distinct connections" shape). Any pipeline failure surfaces as the normal
//!   HTTP error — the stream never starts.
//! - **`@stream(initialCount: N)`** on the single root field of a list query: an initial payload
//!   with `N` items, then continuation batches, each **re-executed through the full executor
//!   pipeline** (GATE-1, authorization, RLS session variables, caching) with paginated
//!   `limit`/`offset` variables. There is deliberately **no** second execution path here: batches
//!   enter through the same `execute`/`execute_with_security` entry points the buffered transport
//!   uses. Variables override inline arguments in the matcher, which is what makes pagination
//!   injectable without rewriting the document — and why a document that declares its own
//!   `$limit`/`$offset` variables is refused loudly rather than silently corrupted.
//!
//! Consistency caveat (shared with the REST export and gRPC streaming paths):
//! batches are separate statements, not one snapshot — concurrent writes can
//! shift rows between batches. Long-lived deliveries re-check principal expiry
//! before every batch (the P18 rule for long-lived responses).

use std::collections::HashMap;

use axum::response::{
    IntoResponse, Response,
    sse::{Event, KeepAlive, Sse},
};
use fraiseql_core::{
    db::traits::DatabaseAdapter,
    graphql::{parse_query, selection_set::variables_map, types::FieldSelection, value_json},
    runtime::QueryMatcher,
    security::SecurityContext,
};
use futures::{StreamExt as _, stream};
use serde_json::{Value, json};
use tracing::{debug, warn};

use super::{
    super::{app_state::AppState, request::GraphQLRequest, tenant_dispatch},
    execute_graphql_request, stages,
};
use crate::error::{ErrorResponse, GraphQLError};

/// Whether the request negotiates the SSE response transport.
///
/// Same `split(',')` idiom as the REST transport's `accepts_sse`; media-type
/// parameters (`;q=…`) are tolerated.
pub(in super::super) fn accepts_event_stream(headers: &axum::http::HeaderMap) -> bool {
    headers
        .get(axum::http::header::ACCEPT)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|accept| {
            accept.split(',').any(|part| {
                part.trim()
                    .split(';')
                    .next()
                    .is_some_and(|media| media.trim().eq_ignore_ascii_case("text/event-stream"))
            })
        })
}

/// A validated plan for one `@stream` delivery.
struct StreamPlan {
    /// Alias-aware response key of the streamed root field.
    response_key:  String,
    /// `initialCount` argument (default 0).
    initial_count: u64,
    /// The client's own `limit` argument, honoured as the total row budget.
    client_limit:  Option<u64>,
    /// The client's own `offset` argument (start position).
    client_offset: u64,
}

/// Serve a negotiated SSE request. Returns the streaming response, or the
/// ordinary HTTP error when the request fails before any event is emitted.
pub(in super::super) async fn handle_sse<A: DatabaseAdapter + Clone + Send + Sync + 'static>(
    state: AppState<A>,
    headers: axum::http::HeaderMap,
    peer_ip: String,
    security_context: Option<SecurityContext>,
    mut request: GraphQLRequest,
) -> Result<Response, ErrorResponse> {
    // Resolve who is asking and what they are asking, through the same stages
    // the buffered pipeline starts with (APQ / trusted-document resolution must
    // happen before the document can be inspected for @stream).
    let security_context =
        Box::pin(stages::authenticate(&state, &headers, security_context)).await?;
    let query = Box::pin(stages::resolve_query_body(&state, &mut request)).await?;

    let plan = plan_stream(&state, &query, request.variables.as_ref())?;

    let Some(plan) = plan else {
        // Single-result mode: the whole buffered pipeline runs once (it
        // re-validates the already-resolved principal, which is idempotent),
        // and the response becomes one `next` event plus `complete`.
        let response = Box::pin(execute_graphql_request(
            state,
            request,
            None,
            security_context,
            &headers,
            &peer_ip,
        ))
        .await?;
        let events = vec![
            next_event(&response.body),
            Event::default().event("complete").data(""),
        ];
        return Ok(sse_response(stream::iter(events.into_iter().map(Ok))));
    };

    // ── @stream mode: one-time gates, then batched delivery ─────────────────
    // Mirrors the one-time prefix of `execute_graphql_request`; per-request
    // stages that only apply to mutations (idempotency, before-mutation hooks)
    // are structurally inapplicable — @stream is queries-only by construction.
    stages::enforce_introspection_policy(&state, &query, security_context.as_ref())?;
    stages::validate_request(&state, &query, &request, &peer_ip)?;

    let tenant_key =
        tenant_dispatch::resolve_tenant_key(&state, security_context.as_ref(), &headers).map_err(
            |e| {
                ErrorResponse::from_error(GraphQLError::new(
                    e.to_string(),
                    crate::error::ErrorCode::ValidationError,
                ))
            },
        )?;

    state.metrics.queries_total.fetch_add(1, std::sync::atomic::Ordering::Relaxed);

    // Cost is charged once for the logical operation; every batch additionally
    // passes the executor's own GATE-1 / max_operation_cost checks.
    {
        let dispatch = tenant_dispatch::dispatch_to_tenant(&state, tenant_key.as_deref())
            .map_err(|e| ErrorResponse::from_error(super::tenant_dispatch_error(&e)))?;
        let estimated_cost = tenant_dispatch::estimate_request_cost(
            &query,
            request.variables.as_ref(),
            &dispatch.executor,
        );
        tenant_dispatch::charge_cost_budget(&state, tenant_key.as_deref(), estimated_cost)
            .map_err(|e| ErrorResponse::from_error(super::tenant_dispatch_error(&e)))?;
    }

    let base_variables = match request.variables.clone() {
        Some(Value::Object(map)) => map,
        _ => serde_json::Map::new(),
    };

    // First batch runs BEFORE the stream is constructed so pre-stream failures
    // (authorization, RLS refusal, bad arguments) surface as ordinary HTTP
    // errors instead of a 200 stream that opens and immediately errors.
    let initial_requested = plan
        .client_limit
        .map_or(plan.initial_count, |total| plan.initial_count.min(total));
    let initial_vars = batch_variables(&base_variables, initial_requested, plan.client_offset);
    let first =
        run_batch(&state, tenant_key.as_deref(), &query, &initial_vars, security_context.as_ref())
            .await
            .map_err(ErrorResponse::from_error)?;

    let initial_rows = extract_items(&first, &plan.response_key).map_or(0, <[Value]>::len) as u64;
    let delivered = initial_rows;
    // `!=` rather than `<`: an over-full batch means pagination is not binding,
    // and continuing would stream forever (see the same guard in `batch_step`).
    let exhausted = initial_rows != initial_requested
        || plan.client_limit.is_some_and(|total| delivered >= total);

    let mut initial_payload = first;
    attach_has_next(&mut initial_payload, !exhausted);

    debug!(
        response_key = %plan.response_key,
        initial_rows,
        exhausted,
        "GraphQL @stream delivery started"
    );

    let batch_size = u64::from(state.graphql_sse_batch_size.max(1));
    let unfold_state = BatchState {
        state: state.clone(),
        query,
        base_variables,
        security_context,
        tenant_key,
        response_key: plan.response_key,
        client_limit: plan.client_limit,
        batch_size,
        offset: plan.client_offset + delivered,
        phase: if exhausted {
            Phase::Complete
        } else {
            Phase::Streaming
        },
        emitted_rows: initial_rows,
        batches: 1,
    };

    let events = stream::iter(vec![Ok(next_event(&initial_payload))])
        .chain(stream::unfold(unfold_state, batch_step));

    Ok(sse_response(events))
}

/// Delivery phases for the continuation stream.
enum Phase {
    /// More batches to fetch.
    Streaming,
    /// Final payload emitted (or a mid-stream error) — emit `complete` next.
    Complete,
    /// Stream finished.
    Finished,
}

/// State threaded through the continuation `unfold`.
struct BatchState<A: DatabaseAdapter + Clone + Send + Sync + 'static> {
    state:            AppState<A>,
    query:            String,
    base_variables:   serde_json::Map<String, Value>,
    security_context: Option<SecurityContext>,
    tenant_key:       Option<String>,
    response_key:     String,
    client_limit:     Option<u64>,
    batch_size:       u64,
    /// Absolute row offset of the next batch.
    offset:           u64,
    phase:            Phase,
    /// Total rows emitted so far (audit).
    emitted_rows:     u64,
    /// Total batches executed so far (audit).
    batches:          u64,
}

/// One step of the continuation stream: fetch and emit the next batch, the
/// final `complete` event, or end the stream.
async fn batch_step<A: DatabaseAdapter + Clone + Send + Sync + 'static>(
    mut st: BatchState<A>,
) -> Option<(Result<Event, std::convert::Infallible>, BatchState<A>)> {
    match st.phase {
        Phase::Finished => None,
        Phase::Complete => {
            st.phase = Phase::Finished;
            st.state
                .metrics
                .queries_success
                .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            // #387 acceptance: the audit trail records the delivery as ONE
            // logical request, with its streaming-event counts.
            tracing::info!(
                target: "fraiseql::sse_audit",
                batches = st.batches,
                rows = st.emitted_rows,
                tenant = st.tenant_key.as_deref().unwrap_or(""),
                "@stream delivery complete"
            );
            Some((Ok(Event::default().event("complete").data("")), st))
        },
        Phase::Streaming => {
            // P18: a long-lived response re-checks its principal before every
            // continuation batch; an expired token terminates the delivery.
            if st.security_context.as_ref().is_some_and(SecurityContext::is_expired) {
                warn!("@stream delivery terminated: principal token expired mid-stream");
                st.phase = Phase::Complete;
                let payload = json!({
                    "errors": [{
                        "message": "Authentication token expired during streaming delivery",
                        "extensions": {"code": "UNAUTHENTICATED"}
                    }],
                    "hasNext": false,
                });
                return Some((Ok(next_event(&payload)), st));
            }

            let remaining = st.client_limit.map(|total| total.saturating_sub(st.emitted_rows));
            let requested = remaining.map_or(st.batch_size, |r| r.min(st.batch_size));
            if requested == 0 {
                st.phase = Phase::Complete;
                return Some((Ok(next_event(&json!({"hasNext": false}))), st));
            }

            let vars = batch_variables(&st.base_variables, requested, st.offset);
            let result = run_batch(
                &st.state,
                st.tenant_key.as_deref(),
                &st.query,
                &vars,
                st.security_context.as_ref(),
            )
            .await;
            st.batches += 1;

            match result {
                Err(err) => {
                    st.phase = Phase::Complete;
                    st.state
                        .metrics
                        .queries_error
                        .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    let err_json = serde_json::to_value(&err)
                        .unwrap_or_else(|_| json!({"message": "streamed batch failed"}));
                    let payload = json!({
                        "errors": [err_json],
                        "hasNext": false,
                    });
                    Some((Ok(next_event(&payload)), st))
                },
                Ok(response) => {
                    let items = extract_items(&response, &st.response_key)
                        .map(<[Value]>::to_vec)
                        .unwrap_or_default();
                    let count = items.len() as u64;
                    let path_index = st.offset;
                    st.offset += count;
                    st.emitted_rows += count;

                    // Fail-safe: a batch larger than it asked for means the
                    // pagination injection is not binding (a broken auto_params
                    // declaration, say). Without this, `count == rows_total`
                    // forever and the stream never terminates.
                    if count > requested {
                        warn!(
                            count,
                            requested,
                            "@stream batch returned more rows than requested; \
                             pagination is not binding — terminating the delivery"
                        );
                    }
                    let done = count != requested
                        || st.client_limit.is_some_and(|total| st.emitted_rows >= total);
                    if done {
                        st.phase = Phase::Complete;
                    }
                    let payload = json!({
                        "incremental": [{
                            "items": items,
                            "path": [st.response_key, path_index],
                        }],
                        "hasNext": !done,
                    });
                    Some((Ok(next_event(&payload)), st))
                },
            }
        },
    }
}

/// Execute one batch through the SAME executor entry points the buffered
/// transport uses — GATE-1, authorization, RLS session variables and the result
/// cache all re-apply per batch. No second execution path.
async fn run_batch<A: DatabaseAdapter + Clone + Send + Sync + 'static>(
    state: &AppState<A>,
    tenant_key: Option<&str>,
    query: &str,
    variables: &Value,
    security_context: Option<&SecurityContext>,
) -> Result<Value, GraphQLError> {
    let dispatch = tenant_dispatch::dispatch_to_tenant(state, tenant_key)
        .map_err(|e| super::tenant_dispatch_error(&e))?;
    let executor = &dispatch.executor;

    let result = if let Some(ctx) = security_context {
        executor.execute_with_security(query, Some(variables), ctx).await
    } else {
        executor.execute(query, Some(variables)).await
    };

    #[allow(unused_mut)]
    // Reason: mut is required by decrypt_response_fields under the secrets feature
    let mut response = result
        .map_err(|e| state.error_sanitizer.sanitize(GraphQLError::from_fraiseql_error(&e)))?;

    #[cfg(feature = "secrets")]
    stages::decrypt_response_fields(state, &mut response)
        .await
        .map_err(|_| GraphQLError::internal("response post-processing failed"))?;

    Ok(response)
}

/// Merge injected pagination into the client's variables.
fn batch_variables(base: &serde_json::Map<String, Value>, limit: u64, offset: u64) -> Value {
    let mut vars = base.clone();
    vars.insert("limit".to_string(), json!(limit));
    vars.insert("offset".to_string(), json!(offset));
    Value::Object(vars)
}

/// The streamed list in a batch response, if present.
fn extract_items<'a>(response: &'a Value, response_key: &str) -> Option<&'a [Value]> {
    response.get("data")?.get(response_key)?.as_array().map(Vec::as_slice)
}

/// Inject `hasNext` into a full execution-result payload.
fn attach_has_next(payload: &mut Value, has_next: bool) {
    if let Some(obj) = payload.as_object_mut() {
        obj.insert("hasNext".to_string(), Value::Bool(has_next));
    }
}

/// Build a `next` event carrying a JSON payload.
fn next_event(payload: &Value) -> Event {
    Event::default().event("next").data(payload.to_string())
}

/// Wrap an event stream in the SSE response with keep-alive comments.
fn sse_response<S>(events: S) -> Response
where
    S: futures::Stream<Item = Result<Event, std::convert::Infallible>> + Send + 'static,
{
    Sse::new(events)
        .keep_alive(KeepAlive::new().interval(std::time::Duration::from_secs(15)).text(""))
        .into_response()
}

/// Inspect the operation for a root-level `@stream`.
///
/// Returns `Ok(None)` for single-result mode (no `@stream`, or `if: false`),
/// `Ok(Some(plan))` for a valid streamed delivery, and a loud error for every
/// unsupported placement — an ignored `@stream` on a negotiated SSE request
/// would read as "streaming worked" while silently buffering.
fn plan_stream<A: DatabaseAdapter + Clone + Send + Sync + 'static>(
    state: &AppState<A>,
    query: &str,
    variables: Option<&Value>,
) -> Result<Option<StreamPlan>, ErrorResponse> {
    let bad_request = |msg: &str| {
        ErrorResponse::from_error(GraphQLError::new(
            msg.to_string(),
            crate::error::ErrorCode::ValidationError,
        ))
    };

    // Cheap pre-scan: no @stream anywhere → single-result mode, no matcher build.
    let Ok(parsed) = parse_query(query) else {
        // Malformed documents take the buffered path, which produces the
        // canonical parse error the client expects.
        return Ok(None);
    };
    if !selections_contain_stream(&parsed.selections) {
        return Ok(None);
    }

    if parsed.operation_type != "query" {
        return Err(bad_request("@stream is only supported on query operations"));
    }
    if parsed.variables.iter().any(|v| v.name == "limit" || v.name == "offset") {
        return Err(bad_request(
            "@stream cannot be combined with document variables named $limit or $offset: \
             streaming paginates by injecting those variables. Rename the variables.",
        ));
    }

    // The matcher resolves fragments, @skip/@include and variables exactly as
    // the executor will, so the plan is built against the same effective
    // document the batches will execute.
    let executor = state.executor.load();
    let matcher = QueryMatcher::new(executor.schema().clone());
    let matched = matcher
        .match_query(query, variables)
        .map_err(|e| bad_request(&format!("@stream planning failed: {e}")))?;

    if matched.fields.len() != 1 {
        return Err(bad_request("@stream requires exactly one root field in the operation"));
    }
    let root = matched
        .selections
        .first()
        .ok_or_else(|| bad_request("@stream requires a selected root field"))?;
    let Some(directive) = root.directives.iter().find(|d| d.name == "stream") else {
        return Err(bad_request(
            "@stream is only supported on the root field of the operation (nested \
             @stream is not supported)",
        ));
    };

    let vars_map = variables_map(variables);
    if !stream_enabled(directive, &vars_map).map_err(|msg| bad_request(&msg))? {
        return Ok(None);
    }
    let initial_count = directive_u64_arg(directive, "initialCount", &vars_map)
        .map_err(|msg| bad_request(&msg))?
        .unwrap_or(0);

    let def = &matched.query_def;
    if !def.returns_list {
        return Err(bad_request("@stream requires a list-returning query field"));
    }
    if def.relay {
        return Err(bad_request(
            "@stream is not supported on relay (connection) queries; use cursor \
             pagination instead",
        ));
    }
    if !(def.auto_params.has_limit && def.auto_params.has_offset) {
        return Err(bad_request(
            "@stream requires the query to accept limit and offset parameters",
        ));
    }

    let client_limit = matched.arguments.get("limit").and_then(Value::as_u64);
    let client_offset = matched.arguments.get("offset").and_then(Value::as_u64).unwrap_or(0);

    Ok(Some(StreamPlan {
        response_key: matched.response_key().to_string(),
        initial_count,
        client_limit,
        client_offset,
    }))
}

/// Recursive scan for a `@stream` directive anywhere in a selection tree.
fn selections_contain_stream(selections: &[FieldSelection]) -> bool {
    selections.iter().any(|s| {
        s.directives.iter().any(|d| d.name == "stream")
            || selections_contain_stream(&s.nested_fields)
    })
}

/// Resolve `@stream(if: …)`, defaulting to enabled.
fn stream_enabled(
    directive: &fraiseql_core::graphql::types::Directive,
    variables: &HashMap<String, Value>,
) -> Result<bool, String> {
    match resolve_directive_arg(directive, "if", variables)? {
        None => Ok(true),
        Some(Value::Bool(b)) => Ok(b),
        Some(other) => Err(format!("@stream(if:) must be a Boolean, got {other}")),
    }
}

/// Resolve an integer directive argument.
fn directive_u64_arg(
    directive: &fraiseql_core::graphql::types::Directive,
    name: &str,
    variables: &HashMap<String, Value>,
) -> Result<Option<u64>, String> {
    match resolve_directive_arg(directive, name, variables)? {
        None => Ok(None),
        Some(value) => value
            .as_u64()
            .map(Some)
            .ok_or_else(|| format!("@stream({name}:) must be a non-negative integer, got {value}")),
    }
}

/// Decode a directive argument and resolve a variable reference if present.
fn resolve_directive_arg(
    directive: &fraiseql_core::graphql::types::Directive,
    name: &str,
    variables: &HashMap<String, Value>,
) -> Result<Option<Value>, String> {
    let Some(arg) = directive.arguments.iter().find(|a| a.name == name) else {
        return Ok(None);
    };
    let decoded = value_json::decode(&arg.value_json)
        .map_err(|e| format!("@stream({name}:) could not be decoded: {e}"))?;
    Ok(Some(value_json::resolve_variables(decoded, variables)))
}
