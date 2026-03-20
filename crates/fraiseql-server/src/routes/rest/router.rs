//! Axum router integration for the REST transport.
//!
//! [`rest_router`] builds an axum [`Router`] from a [`RestRouteTable`] and
//! mounts it with middleware (compression, `X-Request-Id`).  Auth, rate
//! limiting, and CORS are applied at the server level and inherited.

use std::sync::Arc;

use axum::{
    Router,
    body::Body,
    extract::{Request, State},
    http::StatusCode,
    response::Response,
    routing::{delete, get, patch, post, put},
};
use fraiseql_core::{
    db::traits::{DatabaseAdapter, MutationCapable},
    runtime::Executor,
};
use serde_json::json;
use tower_http::compression::CompressionLayer;
use tracing::info;

use super::handler::{RestError, RestHandler, RestResponse};
use super::resource::{HttpMethod, RestRouteTable, RouteSource};
use crate::extractors::OptionalSecurityContext;
use crate::routes::graphql::AppState;

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Derive REST configuration, route table, and shared state from the compiled schema.
///
/// Returns `None` if `rest_config` is absent or `enabled` is `false`, or if
/// route derivation fails. This shared helper is used by both [`rest_query_router`]
/// and [`rest_mutation_router`].
///
/// # Errors
///
/// Returns `None` (with a warning log) if the route table cannot be derived.
fn derive_rest_context<A>(
    state: &AppState<A>,
) -> Option<(String, Arc<RestRouteTable>, RestState<A>)>
where
    A: DatabaseAdapter + Clone + Send + Sync + 'static,
{
    let executor = state.executor();
    let schema = executor.schema();

    let config = match &schema.rest_config {
        Some(cfg) if cfg.enabled => cfg.clone(),
        Some(_) => {
            info!("REST transport disabled (rest.enabled = false)");
            return None;
        }
        None => {
            return None;
        }
    };

    let route_table = match RestRouteTable::from_compiled_schema(schema) {
        Ok(rt) => Arc::new(rt),
        Err(e) => {
            tracing::warn!(error = %e, "REST route derivation failed — REST transport disabled");
            return None;
        }
    };

    // Log diagnostics from derivation.
    for diag in &route_table.diagnostics {
        match diag.level {
            super::resource::DiagnosticLevel::Info => {
                tracing::debug!(message = %diag.message, "REST derivation");
            }
            super::resource::DiagnosticLevel::Warning => {
                tracing::warn!(message = %diag.message, "REST derivation");
            }
            super::resource::DiagnosticLevel::Error => {
                tracing::error!(message = %diag.message, "REST derivation");
            }
        }
    }

    let base_path = config.path;
    let idempotency_store =
        super::idempotency::create_store(config.idempotency_ttl_seconds);

    let rest_state = RestState {
        executor: state.executor.load_full(),
        route_table: route_table.clone(),
        idempotency_store,
        #[cfg(feature = "observers")]
        event_transport: state.event_transport.clone(),
    };

    Some((base_path, route_table, rest_state))
}

/// Build an axum [`Router`] for read-only REST endpoints (GET queries and SSE
/// streams) derived from the compiled schema.
///
/// Returns `None` if `rest_config` is absent or `enabled` is `false`, or if
/// route derivation fails.
///
/// Does **not** require `MutationCapable` — suitable for read-only adapters such
/// as `FraiseWireAdapter` and `SqliteAdapter`.
///
/// The returned router is *not* nested — the caller must merge it into the
/// application router.  Middleware applied at the server level (auth, rate
/// limiting, CORS, tracing, body-size limit) is inherited automatically.
///
/// # Errors
///
/// Returns `None` (with a warning log) if the route table cannot be derived.
pub fn rest_query_router<A>(state: AppState<A>) -> Option<Router>
where
    A: DatabaseAdapter + Clone + Send + Sync + 'static,
{
    let (base_path, route_table, rest_state) = derive_rest_context(&state)?;
    let executor = state.executor();
    let schema = executor.schema();

    let mut router = Router::new();

    for resource in &route_table.resources {
        // Register GET routes for queries.
        for route in &resource.routes {
            if route.method == HttpMethod::Get {
                let axum_path = to_axum_path(&base_path, &route.path);
                router = router.route(&axum_path, get(rest_get_handler::<A>));
            }
        }

        // Register SSE stream route: GET /{resource}/stream
        let stream_path = to_axum_path(&base_path, &format!("/{}/stream", resource.name));
        router = router.route(&stream_path, get(rest_sse_handler::<A>));
    }

    // Finalize state and apply compression.
    let mut router = router
        .with_state(rest_state)
        .layer(CompressionLayer::new());

    // Serve OpenAPI specification at {base_path}/openapi.json.
    let openapi_path = format!("{}/openapi.json", base_path.trim_end_matches('/'));
    let openapi_spec = match super::openapi::generate_openapi(schema, &route_table) {
        Ok(spec) => Arc::new(spec),
        Err(e) => {
            tracing::warn!(error = %e, "OpenAPI spec generation failed");
            Arc::new(json!({"error": "OpenAPI generation failed"}))
        }
    };
    router = router.route(
        &openapi_path,
        get(move || {
            let spec = openapi_spec.clone();
            async move { axum::Json((*spec).clone()) }
        }),
    );

    // Log startup summary.
    let resource_count = route_table.resources.len();
    let get_route_count: usize = route_table
        .resources
        .iter()
        .flat_map(|r| &r.routes)
        .filter(|r| r.method == HttpMethod::Get)
        .count();
    let paths: Vec<String> = route_table
        .resources
        .iter()
        .map(|r| format!("{}/{}", base_path, r.name))
        .collect();
    info!(
        resources = resource_count,
        routes = get_route_count,
        base_path = %base_path,
        paths = ?paths,
        "REST query transport enabled (read-only)"
    );

    Some(router)
}

/// Build an axum [`Router`] for all REST endpoints — both read-only (GET, SSE)
/// and mutation (POST, PUT, PATCH, DELETE) routes — derived from the compiled
/// schema.
///
/// Returns `None` if `rest_config` is absent or `enabled` is `false`, or if
/// route derivation fails.
///
/// Requires `MutationCapable` because mutation handlers call
/// `Executor::execute_mutation()` which has the same compile-time bound.
///
/// The returned router is *not* nested — the caller must merge it into the
/// application router.  Middleware applied at the server level (auth, rate
/// limiting, CORS, tracing, body-size limit) is inherited automatically.
///
/// # Errors
///
/// Returns `None` (with a warning log) if the route table cannot be derived.
pub fn rest_router<A>(state: AppState<A>) -> Option<Router>
where
    A: DatabaseAdapter + MutationCapable + Clone + Send + Sync + 'static,
{
    let (base_path, route_table, rest_state) = derive_rest_context(&state)?;
    let executor = state.executor();
    let schema = executor.schema();

    let mut router = Router::new();

    // Track which collection paths already have PATCH/DELETE so we can add
    // bulk operation routes for resources that have update/delete mutations.
    let mut collection_patch_paths = std::collections::HashSet::new();
    let mut collection_delete_paths = std::collections::HashSet::new();

    for resource in &route_table.resources {
        for route in &resource.routes {
            let axum_path = to_axum_path(&base_path, &route.path);
            router = match route.method {
                HttpMethod::Get => router.route(&axum_path, get(rest_get_handler::<A>)),
                HttpMethod::Post => router.route(&axum_path, post(rest_post_handler::<A>)),
                HttpMethod::Put => router.route(&axum_path, put(rest_put_handler::<A>)),
                HttpMethod::Patch => {
                    let collection_path = to_axum_path(&base_path, &format!("/{}", resource.name));
                    collection_patch_paths.insert(collection_path);
                    router.route(&axum_path, patch(rest_patch_handler::<A>))
                }
                HttpMethod::Delete => {
                    let collection_path = to_axum_path(&base_path, &format!("/{}", resource.name));
                    collection_delete_paths.insert(collection_path);
                    router.route(&axum_path, delete(rest_delete_handler::<A>))
                }
            };
        }

        // Register collection-level PATCH route for bulk update if an update
        // mutation exists but no collection PATCH was derived.
        let collection_path = to_axum_path(&base_path, &format!("/{}", resource.name));
        let has_update = resource.routes.iter().any(|r| {
            matches!(&r.source, RouteSource::Mutation { name }
                if state.executor().schema().find_mutation(name)
                    .is_some_and(|m| matches!(m.operation,
                        fraiseql_core::schema::MutationOperation::Update { .. })))
        });
        if has_update && !collection_patch_paths.contains(&collection_path) {
            router = router.route(&collection_path, patch(rest_patch_handler::<A>));
        }

        // Register collection-level DELETE route for bulk delete.
        let has_delete = resource.routes.iter().any(|r| {
            matches!(&r.source, RouteSource::Mutation { name }
                if state.executor().schema().find_mutation(name)
                    .is_some_and(|m| matches!(m.operation,
                        fraiseql_core::schema::MutationOperation::Delete { .. })))
        });
        if has_delete && !collection_delete_paths.contains(&collection_path) {
            router = router.route(&collection_path, delete(rest_delete_handler::<A>));
        }

        // Register SSE stream route: GET /{resource}/stream
        let stream_path = to_axum_path(&base_path, &format!("/{}/stream", resource.name));
        router = router.route(&stream_path, get(rest_sse_handler::<A>));
    }

    // Finalize state and apply compression.
    let mut router = router
        .with_state(rest_state)
        .layer(CompressionLayer::new());

    // Serve OpenAPI specification at {base_path}/openapi.json.
    let openapi_path = format!("{}/openapi.json", base_path.trim_end_matches('/'));
    let openapi_spec = match super::openapi::generate_openapi(schema, &route_table) {
        Ok(spec) => Arc::new(spec),
        Err(e) => {
            tracing::warn!(error = %e, "OpenAPI spec generation failed");
            Arc::new(json!({"error": "OpenAPI generation failed"}))
        }
    };
    router = router.route(
        &openapi_path,
        get(move || {
            let spec = openapi_spec.clone();
            async move { axum::Json((*spec).clone()) }
        }),
    );

    // Log startup summary.
    let resource_count = route_table.resources.len();
    let route_count: usize = route_table.resources.iter().map(|r| r.routes.len()).sum();
    let paths: Vec<String> = route_table
        .resources
        .iter()
        .map(|r| format!("{}/{}", base_path, r.name))
        .collect();
    info!(
        resources = resource_count,
        routes = route_count,
        base_path = %base_path,
        paths = ?paths,
        "REST transport enabled"
    );

    Some(router)
}

// ---------------------------------------------------------------------------
// State
// ---------------------------------------------------------------------------

/// Shared state for REST handlers.
#[derive(Clone)]
struct RestState<A: DatabaseAdapter> {
    executor: Arc<Executor<A>>,
    route_table: Arc<RestRouteTable>,
    idempotency_store: Arc<dyn super::idempotency::IdempotencyStore>,
    /// Optional event transport for SSE streaming (requires `observers` feature).
    #[cfg(feature = "observers")]
    event_transport: Option<Arc<dyn fraiseql_observers::transport::EventTransport>>,
}

// ---------------------------------------------------------------------------
// Axum handlers
// ---------------------------------------------------------------------------

/// GET handler — query execution (single resource or collection).
///
/// Content negotiation:
/// - `Accept: application/x-ndjson` → NDJSON streaming (one JSON object per line)
/// - `Accept: application/json` (default) → standard envelope response
async fn rest_get_handler<A>(
    State(rest): State<RestState<A>>,
    OptionalSecurityContext(security_ctx): OptionalSecurityContext,
    request: Request<Body>,
) -> Response
where
    A: DatabaseAdapter + Clone + Send + Sync + 'static,
{
    let (parts, _body) = request.into_parts();
    let relative_path = strip_base_path(&rest.route_table.base_path, parts.uri.path());
    let query_string = parts.uri.query().unwrap_or("");
    let query_pairs = parse_query_pairs(query_string);
    let query_refs: Vec<(&str, &str)> = query_pairs
        .iter()
        .map(|(k, v)| (k.as_str(), v.as_str()))
        .collect();

    // NDJSON content negotiation
    if super::streaming::accepts_ndjson(&parts.headers) {
        let schema = rest.executor.schema();
        let config = schema.rest_config.as_ref().expect("REST config must exist");
        let handler = RestHandler::new(&rest.executor, schema, config, &rest.route_table);
        let result = super::streaming::handle_ndjson_get(
            &handler,
            &relative_path,
            &query_refs,
            &parts.headers,
            security_ctx.as_ref(),
        )
        .await;

        return match result {
            Ok(ndjson) => {
                let mut builder = Response::builder().status(StatusCode::OK);
                for (key, value) in &ndjson.headers {
                    builder = builder.header(key, value);
                }
                builder
                    .body(ndjson.body.into_body())
                    .unwrap_or_else(|_| {
                        Response::builder()
                            .status(StatusCode::INTERNAL_SERVER_ERROR)
                            .body(Body::empty())
                            .expect("fallback response")
                    })
            }
            Err(rest_err) => rest_result_to_response(Err(rest_err)),
        };
    }

    let schema = rest.executor.schema();
    let config = schema.rest_config.as_ref().expect("REST config must exist");
    let handler = RestHandler::new(&rest.executor, schema, config, &rest.route_table);

    let result = handler
        .handle_get(&relative_path, &query_refs, &parts.headers, security_ctx.as_ref())
        .await;

    rest_result_to_response(result)
}

/// POST handler — create mutation or custom action.
async fn rest_post_handler<A>(
    State(rest): State<RestState<A>>,
    OptionalSecurityContext(security_ctx): OptionalSecurityContext,
    request: Request<Body>,
) -> Response
where
    A: DatabaseAdapter + MutationCapable + Clone + Send + Sync + 'static,
{
    let (parts, body) = request.into_parts();
    let relative_path = strip_base_path(&rest.route_table.base_path, parts.uri.path());

    let body_value = match read_json_body(body).await {
        Ok(v) => v,
        Err(resp) => return resp,
    };

    let schema = rest.executor.schema();
    let config = schema.rest_config.as_ref().expect("REST config must exist");
    let handler = RestHandler::new(&rest.executor, schema, config, &rest.route_table)
        .with_idempotency_store(&rest.idempotency_store);

    let result = handler
        .handle_post(&relative_path, &body_value, &parts.headers, security_ctx.as_ref())
        .await;

    rest_result_to_response(result)
}

/// PUT handler — full update mutation.
async fn rest_put_handler<A>(
    State(rest): State<RestState<A>>,
    OptionalSecurityContext(security_ctx): OptionalSecurityContext,
    request: Request<Body>,
) -> Response
where
    A: DatabaseAdapter + MutationCapable + Clone + Send + Sync + 'static,
{
    let (parts, body) = request.into_parts();
    let relative_path = strip_base_path(&rest.route_table.base_path, parts.uri.path());

    let body_value = match read_json_body(body).await {
        Ok(v) => v,
        Err(resp) => return resp,
    };

    let schema = rest.executor.schema();
    let config = schema.rest_config.as_ref().expect("REST config must exist");
    let handler = RestHandler::new(&rest.executor, schema, config, &rest.route_table);

    let result = handler
        .handle_put(&relative_path, &body_value, &parts.headers, security_ctx.as_ref())
        .await;

    rest_result_to_response(result)
}

/// PATCH handler — partial update mutation or bulk update.
async fn rest_patch_handler<A>(
    State(rest): State<RestState<A>>,
    OptionalSecurityContext(security_ctx): OptionalSecurityContext,
    request: Request<Body>,
) -> Response
where
    A: DatabaseAdapter + MutationCapable + Clone + Send + Sync + 'static,
{
    let (parts, body) = request.into_parts();
    let relative_path = strip_base_path(&rest.route_table.base_path, parts.uri.path());
    let query_string = parts.uri.query().unwrap_or("");
    let query_pairs = parse_query_pairs(query_string);
    let query_refs: Vec<(&str, &str)> = query_pairs
        .iter()
        .map(|(k, v)| (k.as_str(), v.as_str()))
        .collect();

    let body_value = match read_json_body(body).await {
        Ok(v) => v,
        Err(resp) => return resp,
    };

    let schema = rest.executor.schema();
    let config = schema.rest_config.as_ref().expect("REST config must exist");
    let handler = RestHandler::new(&rest.executor, schema, config, &rest.route_table);

    let result = handler
        .handle_patch(&relative_path, &body_value, &query_refs, &parts.headers, security_ctx.as_ref())
        .await;

    rest_result_to_response(result)
}

/// DELETE handler — single-resource delete or bulk delete.
async fn rest_delete_handler<A>(
    State(rest): State<RestState<A>>,
    OptionalSecurityContext(security_ctx): OptionalSecurityContext,
    request: Request<Body>,
) -> Response
where
    A: DatabaseAdapter + MutationCapable + Clone + Send + Sync + 'static,
{
    let (parts, _body) = request.into_parts();
    let relative_path = strip_base_path(&rest.route_table.base_path, parts.uri.path());
    let query_string = parts.uri.query().unwrap_or("");
    let query_pairs = parse_query_pairs(query_string);
    let query_refs: Vec<(&str, &str)> = query_pairs
        .iter()
        .map(|(k, v)| (k.as_str(), v.as_str()))
        .collect();

    let schema = rest.executor.schema();
    let config = schema.rest_config.as_ref().expect("REST config must exist");
    let handler = RestHandler::new(&rest.executor, schema, config, &rest.route_table);

    let result = handler
        .handle_delete(&relative_path, &query_refs, &parts.headers, security_ctx.as_ref())
        .await;

    rest_result_to_response(result)
}

/// SSE handler — stream entity change events in real-time.
///
/// Returns `501 Not Implemented` when the `observers` feature is disabled.
/// Otherwise, streams events for the given resource type via SSE.
async fn rest_sse_handler<A>(
    State(rest): State<RestState<A>>,
    OptionalSecurityContext(security_ctx): OptionalSecurityContext,
    request: Request<Body>,
) -> Response
where
    A: DatabaseAdapter + Clone + Send + Sync + 'static,
{
    let (parts, _body) = request.into_parts();
    let relative_path = strip_base_path(&rest.route_table.base_path, parts.uri.path());

    // Extract resource name from /{resource}/stream path
    let resource_name = match super::sse::extract_stream_resource(&relative_path) {
        Some(name) => name.to_string(),
        None => {
            return rest_result_to_response(Err(super::handler::RestError::not_found(
                "Stream endpoint not found",
            )));
        }
    };

    // Verify the resource exists
    let schema = rest.executor.schema();
    let has_resource = rest
        .route_table
        .resources
        .iter()
        .any(|r| r.name == resource_name);

    if !has_resource {
        return rest_result_to_response(Err(super::handler::RestError::not_found(format!(
            "Resource not found: {resource_name}"
        ))));
    }

    // Check auth if required
    if let Some(config) = &schema.rest_config {
        if config.require_auth && security_ctx.is_none() {
            return rest_result_to_response(Err(super::handler::RestError {
                status: StatusCode::UNAUTHORIZED,
                code: "UNAUTHENTICATED",
                message: "Authentication required".to_string(),
                details: None,
            }));
        }
    }

    // Read heartbeat interval from REST config (or use default).
    let heartbeat_secs = schema
        .rest_config
        .as_ref()
        .map_or(super::sse::DEFAULT_SSE_HEARTBEAT_SECONDS, |c| c.sse_heartbeat_seconds);

    // Check if observers feature is available
    #[cfg(not(feature = "observers"))]
    {
        let _ = heartbeat_secs; // suppress unused warning
        rest_result_to_response(Err(super::sse::observers_not_available()))
    }

    // With observers feature: set up SSE stream with real event subscription.
    #[cfg(feature = "observers")]
    {
        let _last_event_id = super::sse::extract_last_event_id(&parts.headers);
        let heartbeat_interval = std::time::Duration::from_secs(heartbeat_secs);

        // If we have an event transport, subscribe to real entity events.
        if let Some(ref transport) = rest.event_transport {
            let filter = fraiseql_observers::transport::EventFilter {
                entity_type: Some(resource_name.clone()),
                ..Default::default()
            };

            match transport.subscribe(filter).await {
                Ok(event_stream) => {
                    use futures::StreamExt;

                    // Merge entity events with heartbeat ticks.
                    let heartbeat = futures::stream::unfold((), move |()| async move {
                        tokio::time::sleep(heartbeat_interval).await;
                        let event = axum::response::sse::Event::default()
                            .event("ping")
                            .data("");
                        Some((event, ()))
                    });

                    let entity_events = event_stream.filter_map(|result| async move {
                        match result {
                            Ok(entity_event) => {
                                let event_type = super::sse::event_kind_to_sse_type(
                                    entity_event.event_type.as_str(),
                                );
                                let event = axum::response::sse::Event::default()
                                    .event(event_type)
                                    .id(entity_event.id.to_string())
                                    .json_data(&entity_event.data)
                                    .ok()?;
                                Some(event)
                            }
                            Err(e) => {
                                tracing::warn!(error = %e, "SSE event stream error");
                                None
                            }
                        }
                    });

                    // Select between entity events and heartbeat pings.
                    let merged = futures::stream::select(
                        entity_events,
                        heartbeat,
                    )
                    .map(Ok::<_, std::convert::Infallible>);

                    let sse = axum::response::sse::Sse::new(merged)
                        .keep_alive(
                            axum::response::sse::KeepAlive::new()
                                .interval(heartbeat_interval)
                                .text(""),
                        );

                    return axum::response::IntoResponse::into_response(sse);
                }
                Err(e) => {
                    tracing::warn!(error = %e, resource = %resource_name, "Failed to subscribe to event stream");
                    return rest_result_to_response(Err(super::handler::RestError {
                        status: StatusCode::SERVICE_UNAVAILABLE,
                        code: "EVENT_STREAM_UNAVAILABLE",
                        message: "Could not connect to event stream".to_string(),
                        details: None,
                    }));
                }
            }
        }

        // Fallback: no event transport configured — heartbeat-only stream.
        let stream = futures::stream::unfold((), move |()| async move {
            tokio::time::sleep(heartbeat_interval).await;
            let event = axum::response::sse::Event::default()
                .event("ping")
                .data("");
            Some((Ok::<_, std::convert::Infallible>(event), ()))
        });

        let sse = axum::response::sse::Sse::new(stream)
            .keep_alive(
                axum::response::sse::KeepAlive::new()
                    .interval(heartbeat_interval)
                    .text(""),
            );

        axum::response::IntoResponse::into_response(sse)
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Convert a `RestRouteTable` path pattern to an axum path pattern.
///
/// Axum 0.8 uses `{param}` syntax natively, so no conversion is needed —
/// just concatenate the base path with the route path.
fn to_axum_path(base_path: &str, route_path: &str) -> String {
    let base = base_path.trim_end_matches('/');
    let relative = route_path.trim_start_matches('/');
    if relative.is_empty() {
        base.to_string()
    } else {
        format!("{base}/{relative}")
    }
}

/// Strip the REST base path prefix from a request path.
fn strip_base_path(base_path: &str, request_path: &str) -> String {
    let base = base_path.trim_end_matches('/');
    let stripped = request_path
        .strip_prefix(base)
        .unwrap_or(request_path);
    if stripped.is_empty() {
        "/".to_string()
    } else {
        stripped.to_string()
    }
}

/// Parse a query string into key-value pairs.
///
/// Handles URL-encoded keys and values (e.g., `name%5Bicontains%5D=alice`
/// becomes `("name[icontains]", "alice")`).
fn parse_query_pairs(query: &str) -> Vec<(String, String)> {
    if query.is_empty() {
        return Vec::new();
    }
    query
        .split('&')
        .filter(|s| !s.is_empty())
        .map(|pair| {
            let (key, value) = pair.split_once('=').unwrap_or((pair, ""));
            (
                urlencoding::decode(key).unwrap_or(std::borrow::Cow::Borrowed(key)).into_owned(),
                urlencoding::decode(value).unwrap_or(std::borrow::Cow::Borrowed(value)).into_owned(),
            )
        })
        .collect()
}

/// Read and parse a JSON request body.
async fn read_json_body(body: Body) -> Result<serde_json::Value, Response> {
    let bytes = match axum::body::to_bytes(body, 1_048_576).await {
        Ok(b) => b,
        Err(_) => {
            return Err(error_response(
                StatusCode::PAYLOAD_TOO_LARGE,
                "PAYLOAD_TOO_LARGE",
                "Request body too large",
            ));
        }
    };

    if bytes.is_empty() {
        return Ok(serde_json::Value::Object(serde_json::Map::new()));
    }

    serde_json::from_slice(&bytes).map_err(|e| {
        error_response(
            StatusCode::BAD_REQUEST,
            "INVALID_JSON",
            &format!("Invalid JSON body: {e}"),
        )
    })
}

/// Convert a `RestResponse` or `RestError` to an axum `Response`.
fn rest_result_to_response(result: Result<RestResponse, RestError>) -> Response {
    match result {
        Ok(rest_resp) => {
            let status = rest_resp.status;
            let mut builder = Response::builder().status(status);

            for (key, value) in &rest_resp.headers {
                builder = builder.header(key, value);
            }

            match rest_resp.body {
                Some(body) => {
                    builder = builder.header("content-type", "application/json");
                    let body_bytes = serde_json::to_vec(&body).unwrap_or_default();
                    builder.body(Body::from(body_bytes)).unwrap_or_else(|_| {
                        Response::builder()
                            .status(StatusCode::INTERNAL_SERVER_ERROR)
                            .body(Body::empty())
                            .expect("fallback response")
                    })
                }
                None => builder.body(Body::empty()).unwrap_or_else(|_| {
                    Response::builder()
                        .status(StatusCode::INTERNAL_SERVER_ERROR)
                        .body(Body::empty())
                        .expect("fallback response")
                }),
            }
        }
        Err(rest_err) => {
            let body = rest_err.to_json();
            let body_bytes = serde_json::to_vec(&body).unwrap_or_default();
            let builder = Response::builder()
                .status(rest_err.status)
                .header("content-type", "application/json");

            builder.body(Body::from(body_bytes)).unwrap_or_else(|_| {
                Response::builder()
                    .status(StatusCode::INTERNAL_SERVER_ERROR)
                    .body(Body::empty())
                    .expect("fallback response")
            })
        }
    }
}

/// Build a JSON error response.
fn error_response(status: StatusCode, code: &str, message: &str) -> Response {
    let body = json!({
        "error": {
            "code": code,
            "message": message,
        }
    });
    let body_bytes = serde_json::to_vec(&body).unwrap_or_default();
    Response::builder()
        .status(status)
        .header("content-type", "application/json")
        .body(Body::from(body_bytes))
        .expect("error response")
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
#[allow(clippy::unwrap_used)] // Reason: test code
mod tests {
    use super::*;

    use std::sync::Arc;

    use fraiseql_core::schema::{
        FieldType, MutationDefinition, MutationOperation, RestConfig,
    };
    use fraiseql_test_utils::schema_builder::{
        TestFieldBuilder, TestSchemaBuilder, TestTypeBuilder,
    };

    fn mutation(name: &str, op: MutationOperation) -> MutationDefinition {
        let mut m = MutationDefinition::new(name, "User");
        m.operation = op;
        m.sql_source = Some(format!("fn_{name}"));
        m
    }

    /// Build a minimal schema with REST enabled and one resource (`users`).
    fn schema_with_rest() -> fraiseql_core::schema::CompiledSchema {
        let table = "users".to_string();
        let mut schema = TestSchemaBuilder::new()
            .with_simple_query("users", "User", true)
            .with_simple_query("user", "User", false)
            .with_mutation(mutation("create_user", MutationOperation::Insert { table: table.clone() }))
            .with_mutation(mutation("update_user", MutationOperation::Update { table: table.clone() }))
            .with_mutation(mutation("delete_user", MutationOperation::Delete { table }))
            .with_type(
                TestTypeBuilder::new("User", "v_user")
                    .with_field(TestFieldBuilder::new("pk_user_id", FieldType::Int).build())
                    .with_field(TestFieldBuilder::new("name", FieldType::String).build())
                    .with_field(
                        TestFieldBuilder::nullable("email", FieldType::String).build(),
                    )
                    .build(),
            )
            .build();

        schema.rest_config = Some(RestConfig {
            enabled: true,
            ..RestConfig::default()
        });

        schema
    }

    /// Build a schema with REST disabled.
    fn schema_with_rest_disabled() -> fraiseql_core::schema::CompiledSchema {
        let mut schema = schema_with_rest();
        schema.rest_config = Some(RestConfig {
            enabled: false,
            ..RestConfig::default()
        });
        schema
    }

    /// Build a schema with no REST config at all.
    fn schema_without_rest() -> fraiseql_core::schema::CompiledSchema {
        TestSchemaBuilder::new()
            .with_simple_query("users", "User", true)
            .with_type(
                TestTypeBuilder::new("User", "v_user")
                    .with_field(TestFieldBuilder::new("pk_user_id", FieldType::Int).build())
                    .build(),
            )
            .build()
    }

    fn make_app_state(
        schema: fraiseql_core::schema::CompiledSchema,
    ) -> AppState<fraiseql_test_utils::failing_adapter::FailingAdapter> {
        let adapter = Arc::new(fraiseql_test_utils::failing_adapter::FailingAdapter::default());
        let executor = Arc::new(
            fraiseql_core::runtime::Executor::new(schema, adapter),
        );
        AppState::new(executor)
    }

    // -----------------------------------------------------------------------
    // rest_query_router function tests
    // -----------------------------------------------------------------------

    #[test]
    fn rest_query_router_returns_none_when_no_config() {
        let state = make_app_state(schema_without_rest());
        assert!(rest_query_router(state).is_none());
    }

    #[test]
    fn rest_query_router_returns_none_when_disabled() {
        let state = make_app_state(schema_with_rest_disabled());
        assert!(rest_query_router(state).is_none());
    }

    #[test]
    fn rest_query_router_returns_some_when_enabled() {
        let state = make_app_state(schema_with_rest());
        assert!(rest_query_router(state).is_some());
    }

    // -----------------------------------------------------------------------
    // rest_router function tests
    // -----------------------------------------------------------------------

    #[test]
    fn rest_router_returns_none_when_no_config() {
        let state = make_app_state(schema_without_rest());
        assert!(rest_router(state).is_none());
    }

    #[test]
    fn rest_router_returns_none_when_disabled() {
        let state = make_app_state(schema_with_rest_disabled());
        assert!(rest_router(state).is_none());
    }

    #[test]
    fn rest_router_returns_some_when_enabled() {
        let state = make_app_state(schema_with_rest());
        assert!(rest_router(state).is_some());
    }

    #[test]
    fn rest_router_custom_base_path() {
        let mut schema = schema_with_rest();
        schema.rest_config = Some(RestConfig {
            enabled: true,
            path: "/api/rest".to_string(),
            ..RestConfig::default()
        });
        let state = make_app_state(schema);
        // Should succeed — custom path doesn't prevent creation.
        assert!(rest_router(state).is_some());
    }

    // -----------------------------------------------------------------------
    // Path conversion tests
    // -----------------------------------------------------------------------

    #[test]
    fn to_axum_path_collection() {
        let result = to_axum_path("/rest/v1", "/users");
        assert_eq!(result, "/rest/v1/users");
    }

    #[test]
    fn to_axum_path_single_resource() {
        let result = to_axum_path("/rest/v1", "/users/{id}");
        assert_eq!(result, "/rest/v1/users/{id}");
    }

    #[test]
    fn to_axum_path_action() {
        let result = to_axum_path("/rest/v1", "/users/{id}/archive");
        assert_eq!(result, "/rest/v1/users/{id}/archive");
    }

    #[test]
    fn to_axum_path_trailing_slash_base() {
        let result = to_axum_path("/rest/v1/", "/users");
        assert_eq!(result, "/rest/v1/users");
    }

    #[test]
    fn strip_base_path_normal() {
        let result = strip_base_path("/rest/v1", "/rest/v1/users");
        assert_eq!(result, "/users");
    }

    #[test]
    fn strip_base_path_with_id() {
        let result = strip_base_path("/rest/v1", "/rest/v1/users/123");
        assert_eq!(result, "/users/123");
    }

    #[test]
    fn strip_base_path_root() {
        let result = strip_base_path("/rest/v1", "/rest/v1");
        assert_eq!(result, "/");
    }

    #[test]
    fn parse_query_pairs_empty() {
        let result = parse_query_pairs("");
        assert!(result.is_empty());
    }

    #[test]
    fn parse_query_pairs_simple() {
        let result = parse_query_pairs("limit=10&offset=0");
        assert_eq!(result.len(), 2);
        assert_eq!(result[0], ("limit".to_string(), "10".to_string()));
        assert_eq!(result[1], ("offset".to_string(), "0".to_string()));
    }

    #[test]
    fn parse_query_pairs_encoded() {
        let result = parse_query_pairs("name%5Bicontains%5D=alice");
        assert_eq!(result.len(), 1);
        assert_eq!(
            result[0],
            ("name[icontains]".to_string(), "alice".to_string())
        );
    }
}
