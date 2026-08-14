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
//! shift rows between batches. Long-lived deliveries re-check the principal before
//! every batch — expiry **and** revocation, through the same
//! [`StreamAuthGuard`](crate::middleware::stream_auth::StreamAuthGuard) the
//! subscription transport uses (the P18 rule for long-lived responses, #958).

use std::collections::HashMap;

use axum::response::Response;
use fraiseql_core::{
    db::traits::DatabaseAdapter,
    graphql::{
        defer, parse_query, selection_set, selection_set::variables_map, stream_split,
        types::FieldSelection, value_json,
    },
    runtime::QueryMatcher,
    security::SecurityContext,
};
use futures::{StreamExt as _, stream};
use serde_json::{Value, json};
use tracing::{debug, warn};

use super::{
    super::{app_state::AppState, request::GraphQLRequest, tenant_dispatch},
    execute_graphql_request,
    incremental::{self, Chunk, Wire},
    stages,
};
use crate::{
    error::{ErrorResponse, GraphQLError},
    middleware::stream_auth::StreamAuthGuard,
};

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
    wire: Wire,
    headers: axum::http::HeaderMap,
    peer_ip: String,
    security_context: Option<SecurityContext>,
    token_claims: Option<crate::middleware::oidc_auth::SessionTokenClaims>,
    mut request: GraphQLRequest,
) -> Result<Response, ErrorResponse> {
    // Resolve who is asking and what they are asking, through the same stages
    // the buffered pipeline starts with (APQ / trusted-document resolution must
    // happen before the document can be inspected for @stream).
    let security_context =
        Box::pin(stages::authenticate(&state, &headers, security_context)).await?;
    let query = Box::pin(stages::resolve_query_body(&state, &mut request)).await?;

    let plan = plan_stream(&state, &query, request.variables.as_ref())?;
    let defer_plan = plan_defer(&query, request.variables.as_ref());
    let nested_stream_plan = plan_nested_stream(&query, request.variables.as_ref());

    if plan.is_some() && defer_plan.is_some() {
        return Err(ErrorResponse::from_error(GraphQLError::new(
            "@defer cannot be combined with @stream in one operation: the two order the \
             same response differently and interleaving them is not defined here"
                .to_string(),
            crate::error::ErrorCode::ValidationError,
        )));
    }
    if nested_stream_plan.is_some() && defer_plan.is_some() {
        return Err(ErrorResponse::from_error(GraphQLError::new(
            "@defer cannot be combined with a nested @stream in one operation: both split \
             the delivery of one result and their payload order is not defined here"
                .to_string(),
            crate::error::ErrorCode::ValidationError,
        )));
    }
    if plan.is_some() && nested_stream_plan.is_some() {
        return Err(ErrorResponse::from_error(GraphQLError::new(
            "a root @stream cannot be combined with a nested @stream in one operation: the \
             root one pages the database and each of its batches would carry its own copy \
             of the nested list, which has no incremental addressing"
                .to_string(),
            crate::error::ErrorCode::ValidationError,
        )));
    }

    let Some(plan) = plan else {
        // Single-result mode: the whole buffered pipeline runs once (it
        // re-validates the already-resolved principal, which is idempotent),
        // and the response becomes one `next` event plus `complete`.
        let variables = request.variables.clone();
        let batch_size = state.graphql_incremental_batch_size.max(1) as usize;
        let response = Box::pin(execute_graphql_request(
            state,
            request,
            None,
            security_context,
            &headers,
            &peer_ip,
        ))
        .await?;
        let chunks = match (defer_plan, nested_stream_plan) {
            (Some(selections), _) => {
                deferred_chunks(response.body, &selections, variables.as_ref())
            },
            (None, Some(selections)) => {
                streamed_chunks(response.body, &selections, variables.as_ref(), batch_size)?
            },
            (None, None) => vec![Chunk {
                payload:   response.body,
                resume_id: None,
            }],
        };
        return Ok(incremental::respond(wire, stream::iter(chunks)));
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

    // #958: resume a dropped delivery. Every `next` event carries `id:` = the absolute
    // offset of the first row it has *not* delivered, so `Last-Event-ID` is exactly
    // where to start again. No replay buffer is needed and none would be honest: the
    // source is a re-executable paginated query, not a transient event feed.
    let resume_from = resume_offset(&headers, &plan).map_err(|msg| {
        ErrorResponse::from_error(GraphQLError::new(msg, crate::error::ErrorCode::ValidationError))
    })?;
    let start_offset = resume_from.unwrap_or(plan.client_offset);
    // Rows the earlier connection already delivered, charged against the client's own
    // `limit`: resuming must not restart the row budget, or a resuming client outlives
    // the limit it asked for.
    let already_delivered = start_offset - plan.client_offset;
    let remaining_budget = plan.client_limit.map(|total| total.saturating_sub(already_delivered));

    // First batch runs BEFORE the stream is constructed so pre-stream failures
    // (authorization, RLS refusal, bad arguments) surface as ordinary HTTP
    // errors instead of a 200 stream that opens and immediately errors.
    let initial_requested =
        remaining_budget.map_or(plan.initial_count, |left| plan.initial_count.min(left));
    let initial_vars = batch_variables(&base_variables, initial_requested, start_offset);
    let first =
        run_batch(&state, tenant_key.as_deref(), &query, &initial_vars, security_context.as_ref())
            .await
            .map_err(ErrorResponse::from_error)?;

    let initial_rows = extract_items(&first, &plan.response_key).map_or(0, <[Value]>::len) as u64;
    let delivered = already_delivered + initial_rows;
    // `!=` rather than `<`: an over-full batch means pagination is not binding,
    // and continuing would stream forever (see the same guard in `batch_step`).
    let exhausted = initial_rows != initial_requested
        || plan.client_limit.is_some_and(|total| delivered >= total);

    let mut initial_payload = first;
    attach_has_next(&mut initial_payload, !exhausted);

    debug!(
        response_key = %plan.response_key,
        initial_rows,
        resume_from = ?resume_from,
        exhausted,
        "GraphQL @stream delivery started"
    );

    // #958: the delivery outlives the request that authorised it, so every
    // continuation batch re-checks the principal — expiry AND revocation, through the
    // guard the subscription transport uses. Expiry alone (all this loop did before)
    // leaves a "log out everywhere" and a stolen-token revocation unenforced for the
    // whole life of the delivery, which on a large result set is unbounded.
    let auth_guard = StreamAuthGuard::new(
        security_context.as_ref(),
        token_claims,
        state.revocation_manager.clone(),
    );

    let batch_size = u64::from(state.graphql_incremental_batch_size.max(1));
    let unfold_state = BatchState {
        state: state.clone(),
        query,
        base_variables,
        security_context,
        auth_guard,
        tenant_key,
        response_key: plan.response_key,
        client_limit: plan.client_limit,
        batch_size,
        offset: start_offset + initial_rows,
        phase: if exhausted {
            Phase::Complete
        } else {
            Phase::Streaming
        },
        emitted_rows: delivered,
        batches: 1,
    };

    let resume_id = start_offset + initial_rows;
    let chunks = stream::iter(vec![Chunk {
        payload:   initial_payload,
        resume_id: Some(resume_id),
    }])
    .chain(stream::unfold(unfold_state, batch_step));

    Ok(incremental::respond(wire, chunks))
}

/// The effective selection set of a document that carries an enabled `@defer`,
/// or `None` when it carries none.
///
/// Spreads are expanded and `@skip`/`@include` applied through the same
/// `selection_set` helpers the executor uses, so the tree the split walks is the tree
/// the response was built from. A document that does not parse takes the buffered
/// path, which produces the canonical parse error — same rule as `plan_stream`.
fn plan_defer(query: &str, variables: Option<&Value>) -> Option<Vec<FieldSelection>> {
    let parsed = parse_query(query).ok()?;
    let vars = variables_map(variables);
    let effective =
        selection_set::resolve_and_filter(&parsed.selections, &parsed.fragments, &vars).ok()?;
    defer::contains_defer(&effective, &vars).then_some(effective)
}

/// The effective selection set of a document carrying a **nested** `@stream`, or
/// `None` when it carries none.
///
/// Mirrors [`plan_defer`], including its rule for an unparseable document, because
/// nested `@stream` is the same kind of thing: a split of a delivery the server
/// already holds, not a second query. See
/// [`fraiseql_core::graphql::stream_split`] for why a nested list cannot be paged
/// the way a root one can.
fn plan_nested_stream(query: &str, variables: Option<&Value>) -> Option<Vec<FieldSelection>> {
    let parsed = parse_query(query).ok()?;
    let vars = variables_map(variables);
    let effective =
        selection_set::resolve_and_filter(&parsed.selections, &parsed.fragments, &vars).ok()?;
    stream_split::contains_nested_stream(&effective, &vars).then_some(effective)
}

/// Split `body` into its immediate payload and one event per streamed chunk.
///
/// The whole result is in hand, so the split cannot mis-align: one statement
/// produced the list and every chunk of it. A `@stream` on a field that did not
/// resolve to a list is refused here — before any byte of the response is written,
/// which is why it can still be an ordinary HTTP error.
fn streamed_chunks(
    mut body: Value,
    selections: &[FieldSelection],
    variables: Option<&Value>,
    batch_size: usize,
) -> Result<Vec<Chunk>, ErrorResponse> {
    let vars = variables_map(variables);
    let streamed = match body.get_mut("data") {
        Some(data) => stream_split::split(selections, data, &vars, batch_size).map_err(|e| {
            ErrorResponse::from_error(GraphQLError::new(
                format!("@stream on `{}`: {}", e.field, e.reason),
                crate::error::ErrorCode::ValidationError,
            ))
        })?,
        None => Vec::new(),
    };

    if streamed.is_empty() {
        // Every `@stream` resolved to a list that fits its `initialCount` (or to
        // nothing). The delivery is an ordinary single result, and `hasNext: true`
        // here would leave the client waiting for a payload that will never come.
        return Ok(vec![Chunk {
            payload:   body,
            resume_id: None,
        }]);
    }

    attach_has_next(&mut body, true);
    let mut chunks = vec![Chunk {
        payload:   body,
        resume_id: None,
    }];
    let last = streamed.len() - 1;
    for (index, chunk) in streamed.into_iter().enumerate() {
        chunks.push(Chunk {
            payload:   json!({
                "incremental": [stream_split::incremental_entry(chunk)],
                "hasNext": index != last,
            }),
            resume_id: None,
        });
    }
    Ok(chunks)
}

/// Split `body` into its immediate payload and one event per deferred fragment.
///
/// The whole result is already in hand — `@defer` here changes delivery, not the
/// query plan (see [`fraiseql_core::graphql::defer`]) — so this cannot fail and
/// cannot mis-align: one statement produced every field being split.
fn deferred_chunks(
    mut body: Value,
    selections: &[FieldSelection],
    variables: Option<&Value>,
) -> Vec<Chunk> {
    let vars = variables_map(variables);
    let deferred = body
        .get_mut("data")
        .map(|data| defer::split(selections, data, &vars))
        .unwrap_or_default();

    if deferred.is_empty() {
        // Every `@defer` resolved to a field the response does not carry. The
        // delivery is an ordinary single result, and saying `hasNext: true` here
        // would leave the client waiting for a payload that will never come.
        return vec![Chunk {
            payload:   body,
            resume_id: None,
        }];
    }

    attach_has_next(&mut body, true);
    let mut chunks = vec![Chunk {
        payload:   body,
        resume_id: None,
    }];
    let last = deferred.len() - 1;
    for (index, payload) in deferred.into_iter().enumerate() {
        let mut entry = json!({
            "data": Value::Object(payload.data),
            "path": payload.path,
        });
        if let Some(label) = payload.label {
            entry["label"] = Value::String(label);
        }
        chunks.push(Chunk {
            payload:   json!({
                "incremental": [entry],
                "hasNext": index != last,
            }),
            resume_id: None,
        });
    }
    chunks
}

/// Resolve `Last-Event-ID` into the absolute row offset to resume from.
///
/// Returns `Ok(None)` when the header is absent (a fresh delivery). The id this
/// transport emits is an absolute row offset, so the header is validated as one: it
/// must parse, and it must not point *before* the client's own `offset` argument —
/// that would deliver rows the document did not ask for, turning a reconnect hint into
/// an argument override. A bad value is refused loudly rather than clamped, because a
/// silently-adjusted resume point delivers a wrong result set that looks like a right
/// one.
fn resume_offset(
    headers: &axum::http::HeaderMap,
    plan: &StreamPlan,
) -> Result<Option<u64>, String> {
    let Some(raw) = headers.get("last-event-id").and_then(|v| v.to_str().ok()) else {
        return Ok(None);
    };
    let raw = raw.trim();
    if raw.is_empty() {
        return Ok(None);
    }
    let offset: u64 = raw.parse().map_err(|_| {
        format!(
            "Last-Event-ID must be the absolute row offset this transport emits as the \
             event id, got {raw:?}"
        )
    })?;
    if offset < plan.client_offset {
        return Err(format!(
            "Last-Event-ID {offset} precedes the query's own offset argument \
             ({}); resuming cannot deliver rows before the requested start",
            plan.client_offset
        ));
    }
    Ok(Some(offset))
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
    /// Re-checked before every continuation batch (#958).
    auth_guard:       StreamAuthGuard,
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
) -> Option<(Chunk, BatchState<A>)> {
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
            None
        },
        Phase::Streaming => {
            // P18 + #958: a long-lived response re-checks its principal before every
            // continuation batch — expiry *and* revocation, via the same guard the
            // subscription transport uses. Either terminates the delivery.
            if let Err(reason) = st.auth_guard.check().await {
                warn!(reason, "@stream delivery terminated: principal no longer valid");
                st.phase = Phase::Complete;
                let payload = json!({
                    "errors": [{
                        "message": format!("{reason} during streaming delivery"),
                        "extensions": {"code": "UNAUTHENTICATED"}
                    }],
                    "hasNext": false,
                });
                return Some((resumable_chunk(payload, st.offset), st));
            }

            let remaining = st.client_limit.map(|total| total.saturating_sub(st.emitted_rows));
            let requested = remaining.map_or(st.batch_size, |r| r.min(st.batch_size));
            if requested == 0 {
                st.phase = Phase::Complete;
                return Some((resumable_chunk(json!({"hasNext": false}), st.offset), st));
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
                    Some((resumable_chunk(payload, st.offset), st))
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
                    let resume_id = st.offset;
                    Some((resumable_chunk(payload, resume_id), st))
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

/// A payload stamped with its resume point: the absolute offset of the first row it
/// has **not** delivered (#958).
///
/// Stamping the *next* offset rather than the last delivered one is what makes
/// `Last-Event-ID` directly usable as the resume offset, with no off-by-one for the
/// client to get wrong. The terminal payloads are stamped too — a delivery that ended
/// because the token was revoked is exactly the one a client wants to resume once it
/// holds a fresh one.
const fn resumable_chunk(payload: Value, next_offset: u64) -> Chunk {
    Chunk {
        payload,
        resume_id: Some(next_offset),
    }
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

    // A `@stream` that is not on the root field is a **nested** one: a delivery
    // split over the list the single statement already produced, planned by
    // `plan_nested_stream`. Only a root `@stream` reaches the database paging
    // below, and only it needs the `$limit`/`$offset` injection this refuses to
    // collide with.
    if !parsed
        .selections
        .iter()
        .any(|s| s.directives.iter().any(|d| d.name == "stream"))
    {
        return Ok(None);
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
        // The pre-scan above found a root `@stream` in the parsed document but the
        // matched selection carries none, which means `@skip`/`@include` removed
        // the field that had it. Nothing to page.
        return Ok(None);
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
