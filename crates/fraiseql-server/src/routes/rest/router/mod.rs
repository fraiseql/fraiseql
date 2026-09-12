//! Axum router integration for the REST transport.
//!
//! [`rest_router`] builds an axum [`Router`] from a [`RestRouteTable`] and mounts it
//! with middleware (compression, `X-Request-Id`).
//!
//! **Authentication is attached by the caller, on this router, before it is merged.**
//! `Server::mount_extensions` passes the router through `Server::attach_auth`. This
//! module doc previously claimed auth was "applied at the server level and inherited"
//! — it was not, and could not be: axum's `route_layer` binds to the routes already
//! registered on the same `Router`, and `Router::merge` does not propagate it. Rate
//! limiting and CORS *are* inherited, because those use a global `.layer()`. The
//! discrepancy was #812: every REST request reached its handler with no principal.
//!
//! `RestConfig.require_auth` is enforced for every route by the [`RestSecurityContext`]
//! extractor (#810).

pub mod helpers;

#[cfg(test)]
mod tests;

use std::{future::Future, sync::Arc};

use axum::{
    Router,
    body::Body,
    extract::{FromRequestParts, Request, State},
    http::{StatusCode, request::Parts},
    response::{IntoResponse, Response},
    routing::{delete, get, patch, post, put},
};
use fraiseql_core::{
    db::traits::{DatabaseAdapter, SupportsMutations},
    runtime::Executor,
    security::SecurityContext,
};
use helpers::{
    error_response, parse_query_pairs, rest_result_to_response, strip_base_path, to_axum_path,
};
use serde_json::json;
use tower_http::compression::{CompressionLayer, predicate::SizeAbove};
use tracing::info;

use super::{
    handler::{RestError, RestHandler, RestResponse},
    resource::{HttpMethod, MountedRoutes, RestRouteTable},
};
use crate::{extractors::OptionalSecurityContext, routes::graphql::AppState};

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Everything the REST routers need from the **server** that the compiled schema cannot
/// supply.
///
/// A struct rather than positional arguments because the two flags are both `bool` and
/// were passed as `rest_router(&state, false, false)` at a dozen call sites, where
/// nothing distinguishes "compression off" from "no auth attached". Adding the export
/// config as a third positional would have made that worse.
///
/// [`export`](RestMountConfig::export) arrives here because it is a runtime concern read
/// from `fraiseql.toml`, not a compiled-schema one — the layering rule stated in
/// `routes::rest::export_config`'s module doc. It had no deserialization site at all
/// before #917; every consumer built a fresh `ExportConfig::default()`.
#[derive(Debug, Clone, Default)]
pub struct RestMountConfig {
    /// Apply the framework-level compression layer to REST responses.
    pub compression_enabled: bool,

    /// Whether the caller will attach an authentication layer to the returned router.
    ///
    /// Purely descriptive: it tells the served `OpenAPI` document whether a credential
    /// is required, so the published contract matches what is enforced (#810). It does
    /// not itself attach anything.
    pub auth_layer_attached: bool,

    /// Export-format configuration, from `[export]` in `fraiseql.toml`.
    pub export: Arc<super::export_config::ExportConfig>,
}

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
    // `mount` is read only by the export writers' concurrency limits below, each behind an
    // `export-*` cfg, so a `rest`-without-export build warned that it is unused (#1291).
    // Silenced on the parameter rather than by making the arity depend on features:
    // `derive_rest_context` is called by both router constructors, and a signature that
    // changed shape would push the cfg out to every call site. Not `_mount` either — the
    // name is read in this function under other configurations, and an underscore would
    // have to be undone the moment a third reader appears.
    //
    // ⚠ The warning stood because no gate denied warnings in this configuration: preflight
    // lints `--all-features` (where the export cfgs are compiled in and `mount` IS used),
    // and the feature-matrix combos that build it run `cargo check`. The
    // `server-rest` combo in `.dagger/feature-combos.go` is clippy-enabled for that reason.
    #[cfg_attr(
        not(any(feature = "export-csv", feature = "export-xlsx")),
        allow(unused_variables)
    )]
    mount: &RestMountConfig,
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
        },
        None => {
            return None;
        },
    };

    let route_table = match RestRouteTable::from_compiled_schema(schema) {
        Ok(rt) => Arc::new(rt),
        Err(e) => {
            tracing::warn!(error = %e, "REST route derivation failed — REST transport disabled");
            return None;
        },
    };

    // Log diagnostics from derivation.
    for diag in &route_table.diagnostics {
        match diag.level {
            super::resource::DiagnosticLevel::Info => {
                tracing::debug!(message = %diag.message, "REST derivation");
            },
            super::resource::DiagnosticLevel::Warning => {
                tracing::warn!(message = %diag.message, "REST derivation");
            },
            super::resource::DiagnosticLevel::Error => {
                tracing::error!(message = %diag.message, "REST derivation");
            },
        }
    }

    let base_path = config.path;
    let idempotency_store = super::idempotency::create_store(config.idempotency_ttl_seconds);

    let rest_state = RestState {
        executor: state.executor.load_full(),
        route_table: route_table.clone(),
        idempotency_store,
        error_sanitizer: Arc::clone(&state.error_sanitizer),
        function_hooks: state.before_mutation_hooks.clone(),
        #[cfg(feature = "observers")]
        event_fanout: state.entity_event_fanout.clone(),
        #[cfg(feature = "observers")]
        stream_replay: state.stream_replay.clone(),
        #[cfg(feature = "export-xlsx")]
        xlsx_semaphore: Arc::new(tokio::sync::Semaphore::new(mount.export.max_concurrent_xlsx)),
        #[cfg(any(feature = "export-csv", feature = "export-xlsx"))]
        export: Arc::clone(&mount.export),
    };

    Some((base_path, route_table, rest_state))
}

/// Build an axum [`Router`] for read-only REST endpoints (GET queries and SSE
/// streams) derived from the compiled schema.
///
/// Returns `None` if `rest_config` is absent or `enabled` is `false`, or if
/// route derivation fails.
///
/// Does **not** require `SupportsMutations` — suitable for read-only adapters such
/// as `FraiseWireAdapter` and `SqliteAdapter`.
///
/// The returned router is *not* nested — the caller must merge it into the
/// application router. Rate limiting, CORS, tracing and the body-size limit are
/// applied globally by the server and inherited. **Authentication is not**: axum's
/// `route_layer` does not survive `Router::merge`, so the caller must pass this router
/// through `Server::attach_auth` before merging it (#812).
///
/// `auth_layer_attached` tells the served `OpenAPI` document whether that happened, so
/// the machine-readable contract matches what is enforced (#810).
///
/// # Errors
///
/// Returns `None` (with a warning log) if the route table cannot be derived.
pub fn rest_query_router<A>(state: &AppState<A>, mount: &RestMountConfig) -> Option<Router>
where
    A: DatabaseAdapter + Clone + Send + Sync + 'static,
{
    let (base_path, route_table, rest_state) = derive_rest_context(state, mount)?;
    let executor = state.executor();
    let schema = executor.schema();

    // The surface this router serves, computed once. Registration below iterates it, and
    // the served OpenAPI document is filtered through it, so the router and its published
    // contract cannot describe different sets of operations (#865).
    let mounted = MountedRoutes::read_surface(&route_table);

    let mut router = Router::new();
    for (path, method) in mounted.iter() {
        let axum_path = to_axum_path(&base_path, path);
        // `read_surface` yields GETs only; the SSE routes are the ones ending `/stream`.
        router = if path.ends_with("/stream") {
            router.route(&axum_path, get(rest_sse_handler::<A>))
        } else {
            debug_assert_eq!(method, HttpMethod::Get, "read surface must be GET-only");
            router.route(&axum_path, get(rest_get_handler::<A>))
        };
    }

    // Serve the OpenAPI specification at {base_path}/openapi.json.
    //
    // Registered *before* `with_state` so the handler can take the
    // `RestSecurityContext` extractor and be covered by `require_auth` like every other
    // route. A REST surface closed to anonymous callers should not hand those same
    // callers a full description of its resources, fields and filters; and leaving the
    // one meta route ungated would make the transport's posture non-uniform, which is
    // exactly the per-route drift that #810 was.
    let openapi_path = format!("{}/openapi.json", base_path.trim_end_matches('/'));
    let openapi_spec =
        build_openapi_spec(schema, &route_table, mount.auth_layer_attached, &mounted);
    let router = router.route(&openapi_path, get(serve_openapi(openapi_spec)));

    // Finalize state; apply framework-level compression if enabled.
    let mut router = router.with_state(rest_state);
    if mount.compression_enabled {
        router = router.layer(CompressionLayer::new().compress_when(SizeAbove::new(1024)));
    }

    // Log startup summary.
    let resource_count = route_table.resources.len();
    let get_route_count = mounted.len();
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
/// Requires `SupportsMutations` because mutation handlers call
/// `Executor::execute_mutation()` which has the same compile-time bound.
///
/// The returned router is *not* nested — the caller must merge it into the
/// application router. Rate limiting, CORS, tracing and the body-size limit are
/// applied globally by the server and inherited. **Authentication is not**: axum's
/// `route_layer` does not survive `Router::merge`, so the caller must pass this router
/// through `Server::attach_auth` before merging it (#812).
///
/// `auth_layer_attached` tells the served `OpenAPI` document whether that happened, so
/// the machine-readable contract matches what is enforced (#810).
///
/// # Errors
///
/// Returns `None` (with a warning log) if the route table cannot be derived.
pub fn rest_router<A>(state: &AppState<A>, mount: &RestMountConfig) -> Option<Router>
where
    A: DatabaseAdapter + SupportsMutations + Clone + Send + Sync + 'static,
{
    let (base_path, route_table, rest_state) = derive_rest_context(state, mount)?;
    let executor = state.executor();
    let schema = executor.schema();

    // The full surface — derived routes plus the collection-level bulk fallbacks —
    // computed once and used for both registration and the served document. See
    // `MountedRoutes::write_surface`; #918 was the two answers drifting apart.
    let mounted = MountedRoutes::write_surface(schema, &route_table);

    let mut router = Router::new();

    for (path, method) in mounted.iter() {
        let axum_path = to_axum_path(&base_path, path);
        router = match method {
            HttpMethod::Get if path.ends_with("/stream") => {
                router.route(&axum_path, get(rest_sse_handler::<A>))
            },
            HttpMethod::Get => router.route(&axum_path, get(rest_get_handler::<A>)),
            HttpMethod::Post => router.route(&axum_path, post(rest_post_handler::<A>)),
            HttpMethod::Put => router.route(&axum_path, put(rest_put_handler::<A>)),
            HttpMethod::Patch => router.route(&axum_path, patch(rest_patch_handler::<A>)),
            HttpMethod::Delete => router.route(&axum_path, delete(rest_delete_handler::<A>)),
        };
    }

    // Serve the OpenAPI specification at {base_path}/openapi.json.
    //
    // Registered *before* `with_state` so the handler can take the
    // `RestSecurityContext` extractor and be covered by `require_auth` like every other
    // route. A REST surface closed to anonymous callers should not hand those same
    // callers a full description of its resources, fields and filters; and leaving the
    // one meta route ungated would make the transport's posture non-uniform, which is
    // exactly the per-route drift that #810 was.
    let openapi_path = format!("{}/openapi.json", base_path.trim_end_matches('/'));
    let openapi_spec =
        build_openapi_spec(schema, &route_table, mount.auth_layer_attached, &mounted);
    let router = router.route(&openapi_path, get(serve_openapi(openapi_spec)));

    // Finalize state; apply framework-level compression if enabled.
    let mut router = router.with_state(rest_state);
    if mount.compression_enabled {
        router = router.layer(CompressionLayer::new().compress_when(SizeAbove::new(1024)));
    }

    // Log startup summary.
    let resource_count = route_table.resources.len();
    let route_count = mounted.len();
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

/// Refuse a request for an export format the operator has disabled.
///
/// Returns `Some(406)` when `format` is absent from `[export] export_formats`, `None`
/// when it is allowed. One helper for every negotiation path, so the kill-switch cannot
/// be honoured on one and forgotten on another — which is the shape it would have taken,
/// since the CSV, XLSX and Parquet branches each build their own config.
///
/// `406 Not Acceptable` rather than `404`: the resource exists and the route answers, it
/// is the requested *representation* the server declines to produce.
#[cfg(any(feature = "export-csv", feature = "export-xlsx"))]
fn refuse_disabled_export<A: DatabaseAdapter>(
    rest: &RestState<A>,
    format: super::export_config::ExportFormat,
) -> Option<Response> {
    if rest.export.serves(format) {
        return None;
    }
    Some(
        Response::builder()
            .status(StatusCode::NOT_ACCEPTABLE)
            .header("content-type", "application/json")
            .body(Body::from(format!(
                r#"{{"error":{{"code":"EXPORT_FORMAT_DISABLED","message":"export format {format:?} is not enabled; see [export] export_formats"}}}}"#
            )))
            .unwrap_or_else(|_| {
                Response::builder()
                    .status(StatusCode::NOT_ACCEPTABLE)
                    .body(Body::empty())
                    .expect("fallback response with empty body is infallible")
            }),
    )
}

/// Build the served `OpenAPI` document, degrading to an error object rather than
/// refusing to mount the transport if generation fails.
fn build_openapi_spec(
    schema: &fraiseql_core::schema::CompiledSchema,
    route_table: &RestRouteTable,
    auth_layer_attached: bool,
    mounted: &MountedRoutes,
) -> Arc<serde_json::Value> {
    match super::openapi::generate_openapi(schema, route_table, auth_layer_attached, mounted) {
        Ok(spec) => Arc::new(spec),
        Err(e) => {
            tracing::warn!(error = %e, "OpenAPI spec generation failed");
            Arc::new(json!({"error": "OpenAPI generation failed"}))
        },
    }
}

/// Handler for `{base_path}/openapi.json`.
///
/// Takes [`RestSecurityContext`] purely so the meta route carries the same
/// `require_auth` posture as the data routes (#810).
fn serve_openapi(
    spec: Arc<serde_json::Value>,
) -> impl Fn(RestSecurityContext) -> std::future::Ready<axum::Json<serde_json::Value>> + Clone {
    move |RestSecurityContext(_)| std::future::ready(axum::Json((*spec).clone()))
}

// ---------------------------------------------------------------------------
// State
// ---------------------------------------------------------------------------

/// Shared state for REST handlers.
#[derive(Clone)]
struct RestState<A: DatabaseAdapter> {
    executor:          Arc<Executor<A>>,
    route_table:       Arc<RestRouteTable>,
    idempotency_store: Arc<dyn super::idempotency::IdempotencyStore>,
    /// Error sanitizer (from `compiled.security.error_sanitization`). Strips raw DB/SQL
    /// detail from 5xx response bodies before they reach the client (H7).
    error_sanitizer:   Arc<crate::config::error_sanitization::ErrorSanitizer>,
    /// After-mutation function-trigger hooks (#460), forwarded to the mutation
    /// handlers so a committed REST mutation can dispatch `after:mutation`
    /// functions. `None` when the functions subsystem is absent.
    function_hooks:    Option<Arc<crate::subsystems::BeforeMutationHooks>>,
    /// Export-format configuration from `[export]` in `fraiseql.toml` (#917).
    ///
    /// Carried on the state rather than rebuilt per request: the CSV, XLSX and Parquet
    /// paths each used to construct their own `ExportConfig::default()`, so an operator's
    /// delimiter, BOM setting, row caps and format allow-list reached none of them.
    ///
    /// Gated to its readers: with neither export feature compiled in there is no
    /// negotiation path to configure, and an unconditional field would be dead code.
    #[cfg(any(feature = "export-csv", feature = "export-xlsx"))]
    export:            Arc<super::export_config::ExportConfig>,
    /// The entity-event fan-out `/{resource}/stream` reads (#1309).
    ///
    /// This was `Option<Arc<dyn EventTransport>>`, which is the field whose *type*
    /// invited the defect: `EventTransport::subscribe` is a competing consumer on all
    /// three transports, so populating it would have made every open SSE connection
    /// take events away from the observer executor rather than fan out beside it. A
    /// broadcast handle cannot express that mistake — every receiver gets every event,
    /// and the executor is upstream of the fan-out entirely.
    ///
    /// `Some` exactly when an observer runtime is configured (see
    /// `Server::entity_event_fanout`), so the handler can tell "no producer" — answer
    /// 501 — from "a producer with nothing to say yet".
    #[cfg(feature = "observers")]
    event_fanout:      Option<crate::subscriptions::EntityEventFanout>,
    /// Reads back what `/{resource}/stream` already delivered, for a client that
    /// reconnects with `Last-Event-ID` (#1310).
    ///
    /// `Some` exactly when an observer runtime polls the local change log, because the
    /// order it replays is the one that runtime's dispatch ledger recorded. `None` — no
    /// runtime, or one fed from elsewhere — keeps the `501 RESUMPTION_UNSUPPORTED` this
    /// endpoint answered for every resume before, which is then the truth rather than a
    /// placeholder: nothing here knows what that stream delivered.
    #[cfg(feature = "observers")]
    stream_replay:     Option<std::sync::Arc<fraiseql_observers::listener::ChangeLogReplayReader>>,
    /// Concurrency cap for in-flight XLSX workbook builds. Sized at startup
    /// from [`super::export_config::ExportConfig::max_concurrent_xlsx`].
    #[cfg(feature = "export-xlsx")]
    xlsx_semaphore:    Arc<tokio::sync::Semaphore>,
}

// ---------------------------------------------------------------------------
// Security context extraction
// ---------------------------------------------------------------------------

/// The request's [`SecurityContext`], with `RestConfig.require_auth` **enforced**.
///
/// #810 shipped because `require_auth` was a per-handler responsibility and five of the
/// six handlers simply did not discharge it: only `rest_sse_handler` read the flag, so
/// `require_auth = true` closed the stream route and left every data route open, while
/// the served `OpenAPI` advertised `BearerAuth` + 401 on all of them.
///
/// Making the check part of *extracting the context* removes the possibility of
/// forgetting it: a handler cannot obtain a `SecurityContext` without the guard having
/// run, and a handler that does not obtain one cannot execute a query. That is the
/// difference between a rule and a rule that is enforced by the type system.
struct RestSecurityContext(Option<SecurityContext>);

impl<A> FromRequestParts<RestState<A>> for RestSecurityContext
where
    A: DatabaseAdapter + Clone + Send + Sync + 'static,
{
    type Rejection = Response;

    #[allow(clippy::manual_async_fn)] // Reason: axum's FromRequestParts requires an explicit Future type in return position
    fn from_request_parts(
        parts: &mut Parts,
        state: &RestState<A>,
    ) -> impl Future<Output = Result<Self, Self::Rejection>> + Send {
        let require_auth =
            state.executor.schema().rest_config.as_ref().is_some_and(|c| c.require_auth);
        let sanitizer = Arc::clone(&state.error_sanitizer);

        async move {
            let OptionalSecurityContext(security_ctx) =
                OptionalSecurityContext::from_request_parts(parts, &())
                    .await
                    .map_err(IntoResponse::into_response)?;

            if require_auth && security_ctx.is_none() {
                return Err(rest_result_to_response(Err(RestError::unauthenticated()), &sanitizer));
            }

            Ok(Self(security_ctx))
        }
    }
}

// ---------------------------------------------------------------------------
// Axum handlers
// ---------------------------------------------------------------------------

/// GET handler — query execution (single resource or collection).
///
/// Content negotiation:
/// - `Accept: application/x-ndjson` → NDJSON streaming (one JSON object per line)
/// - `Accept: text/csv` → CSV streaming (with `export-csv` feature)
/// - `Accept: application/vnd.openxmlformats-officedocument.spreadsheetml.sheet` → XLSX workbook
///   (with `export-xlsx` feature)
/// - `Accept: application/json` (default) → standard envelope response
async fn rest_get_handler<A>(
    State(rest): State<RestState<A>>,
    RestSecurityContext(security_ctx): RestSecurityContext,
    request: Request<Body>,
) -> Response
where
    A: DatabaseAdapter + Clone + Send + Sync + 'static,
{
    let (parts, _body) = request.into_parts();
    let relative_path = strip_base_path(&rest.route_table.base_path, parts.uri.path());
    let query_string = parts.uri.query().unwrap_or("");
    let query_pairs = parse_query_pairs(query_string);
    let query_refs: Vec<(&str, &str)> =
        query_pairs.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();

    // NDJSON content negotiation
    if super::streaming::accepts_ndjson(&parts.headers) {
        let schema = rest.executor.schema();
        let config = schema.rest_config.as_ref().expect("REST config must exist: handler is only reached via a matched REST route, which requires rest_config to be present in the schema");
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
                builder.body(ndjson.body.into_body()).unwrap_or_else(|_| {
                    Response::builder()
                        .status(StatusCode::INTERNAL_SERVER_ERROR)
                        .body(Body::empty())
                        .expect("fallback response: Response::builder() with INTERNAL_SERVER_ERROR status and empty body is infallible")
                })
            },
            Err(rest_err) => rest_result_to_response(Err(rest_err), &rest.error_sanitizer),
        };
    }

    // XLSX content negotiation (gated by `export-xlsx` feature). Checked
    // before CSV so a client that sends both still gets the more specific
    // workbook format when explicitly requested.
    #[cfg(feature = "export-xlsx")]
    if super::streaming::xlsx::accepts_xlsx(&parts.headers) {
        if let Some(refusal) =
            refuse_disabled_export(&rest, super::export_config::ExportFormat::Xlsx)
        {
            return refusal;
        }
        // Bound concurrent workbook builds. `try_acquire_owned` is
        // non-blocking; over-the-cap requests get an immediate 503 with a
        // `Retry-After: 1` hint rather than queueing.
        let Ok(_permit) = Arc::clone(&rest.xlsx_semaphore).try_acquire_owned() else {
            return Response::builder()
                .status(StatusCode::SERVICE_UNAVAILABLE)
                .header("retry-after", "1")
                .header("content-type", "application/json")
                .body(Body::from(
                    r#"{"error":{"code":"XLSX_BUSY","message":"max concurrent XLSX exports reached; try again shortly"}}"#,
                ))
                .unwrap_or_else(|_| {
                    Response::builder()
                        .status(StatusCode::SERVICE_UNAVAILABLE)
                        .body(Body::empty())
                        .expect("fallback response: Response::builder() with SERVICE_UNAVAILABLE status and empty body is infallible")
                });
        };

        let schema = rest.executor.schema();
        let config = schema.rest_config.as_ref().expect("REST config must exist: handler is only reached via a matched REST route, which requires rest_config to be present in the schema");
        let handler = RestHandler::new(&rest.executor, schema, config, &rest.route_table);
        let export_config = rest.export.as_ref();
        let result = super::streaming::xlsx::handle_xlsx_get(
            &handler,
            export_config,
            &relative_path,
            &query_refs,
            &parts.headers,
            security_ctx.as_ref(),
        )
        .await;

        return match result {
            Ok(xlsx) => {
                let mut builder = Response::builder().status(StatusCode::OK);
                for (key, value) in &xlsx.headers {
                    builder = builder.header(key, value);
                }
                builder.body(xlsx.body.into_body()).unwrap_or_else(|_| {
                    Response::builder()
                        .status(StatusCode::INTERNAL_SERVER_ERROR)
                        .body(Body::empty())
                        .expect("fallback response: Response::builder() with INTERNAL_SERVER_ERROR status and empty body is infallible")
                })
            },
            Err(rest_err) => rest_result_to_response(Err(rest_err), &rest.error_sanitizer),
        };
    }

    // CSV content negotiation (gated by `export-csv` feature).
    #[cfg(feature = "export-csv")]
    if super::streaming::csv::accepts_csv(&parts.headers) {
        if let Some(refusal) =
            refuse_disabled_export(&rest, super::export_config::ExportFormat::Csv)
        {
            return refusal;
        }
        let schema = rest.executor.schema();
        let config = schema.rest_config.as_ref().expect("REST config must exist: handler is only reached via a matched REST route, which requires rest_config to be present in the schema");
        let handler = RestHandler::new(&rest.executor, schema, config, &rest.route_table);
        // #917: the operator's `[export]` table, not a fresh default. The comment that
        // stood here conceded that "TOML-driven `ExportConfig` loading is a later phase".
        let export_config = rest.export.as_ref();
        let result = super::streaming::csv::handle_csv_get(
            &handler,
            export_config,
            &relative_path,
            &query_refs,
            &parts.headers,
            security_ctx.as_ref(),
        )
        .await;

        return match result {
            Ok(csv) => {
                let mut builder = Response::builder().status(StatusCode::OK);
                for (key, value) in &csv.headers {
                    builder = builder.header(key, value);
                }
                builder.body(csv.body.into_body()).unwrap_or_else(|_| {
                    Response::builder()
                        .status(StatusCode::INTERNAL_SERVER_ERROR)
                        .body(Body::empty())
                        .expect("fallback response: Response::builder() with INTERNAL_SERVER_ERROR status and empty body is infallible")
                })
            },
            Err(rest_err) => rest_result_to_response(Err(rest_err), &rest.error_sanitizer),
        };
    }

    let schema = rest.executor.schema();
    let config = schema.rest_config.as_ref().expect("REST config must exist: handler is only reached via a matched REST route, which requires rest_config to be present in the schema");
    let handler = RestHandler::new(&rest.executor, schema, config, &rest.route_table);

    let result = handler
        .handle_get(&relative_path, &query_refs, &parts.headers, security_ctx.as_ref())
        .await;

    rest_result_to_response(result, &rest.error_sanitizer)
}

/// POST handler — create mutation or custom action.
async fn rest_post_handler<A>(
    State(rest): State<RestState<A>>,
    RestSecurityContext(security_ctx): RestSecurityContext,
    request: Request<Body>,
) -> Response
where
    A: DatabaseAdapter + SupportsMutations + Clone + Send + Sync + 'static,
{
    let (parts, body) = request.into_parts();
    let relative_path = strip_base_path(&rest.route_table.base_path, parts.uri.path());

    let body_value = match read_json_body(body).await {
        Ok(v) => v,
        Err(resp) => return resp,
    };

    let schema = rest.executor.schema();
    let config = schema.rest_config.as_ref().expect("REST config must exist: handler is only reached via a matched REST route, which requires rest_config to be present in the schema");
    let handler = RestHandler::new(&rest.executor, schema, config, &rest.route_table)
        .with_idempotency_store(&rest.idempotency_store)
        .with_function_hooks(rest.function_hooks.as_ref());

    let result = handler
        .handle_post(&relative_path, &body_value, &parts.headers, security_ctx.as_ref())
        .await;

    rest_result_to_response(result, &rest.error_sanitizer)
}

/// PUT handler — full update mutation.
async fn rest_put_handler<A>(
    State(rest): State<RestState<A>>,
    RestSecurityContext(security_ctx): RestSecurityContext,
    request: Request<Body>,
) -> Response
where
    A: DatabaseAdapter + SupportsMutations + Clone + Send + Sync + 'static,
{
    let (parts, body) = request.into_parts();
    let relative_path = strip_base_path(&rest.route_table.base_path, parts.uri.path());

    let body_value = match read_json_body(body).await {
        Ok(v) => v,
        Err(resp) => return resp,
    };

    let schema = rest.executor.schema();
    let config = schema.rest_config.as_ref().expect("REST config must exist: handler is only reached via a matched REST route, which requires rest_config to be present in the schema");
    let handler = RestHandler::new(&rest.executor, schema, config, &rest.route_table)
        .with_function_hooks(rest.function_hooks.as_ref());

    let result = handler
        .handle_put(&relative_path, &body_value, &parts.headers, security_ctx.as_ref())
        .await;

    rest_result_to_response(result, &rest.error_sanitizer)
}

/// PATCH handler — partial update mutation or bulk update.
async fn rest_patch_handler<A>(
    State(rest): State<RestState<A>>,
    RestSecurityContext(security_ctx): RestSecurityContext,
    request: Request<Body>,
) -> Response
where
    A: DatabaseAdapter + SupportsMutations + Clone + Send + Sync + 'static,
{
    let (parts, body) = request.into_parts();
    let relative_path = strip_base_path(&rest.route_table.base_path, parts.uri.path());
    let query_string = parts.uri.query().unwrap_or("");
    let query_pairs = parse_query_pairs(query_string);
    let query_refs: Vec<(&str, &str)> =
        query_pairs.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();

    let body_value = match read_json_body(body).await {
        Ok(v) => v,
        Err(resp) => return resp,
    };

    let schema = rest.executor.schema();
    let config = schema.rest_config.as_ref().expect("REST config must exist: handler is only reached via a matched REST route, which requires rest_config to be present in the schema");
    let handler = RestHandler::new(&rest.executor, schema, config, &rest.route_table)
        .with_function_hooks(rest.function_hooks.as_ref());

    let result = handler
        .handle_patch(
            &relative_path,
            &body_value,
            &query_refs,
            &parts.headers,
            security_ctx.as_ref(),
        )
        .await;

    rest_result_to_response(result, &rest.error_sanitizer)
}

/// DELETE handler — single-resource delete or bulk delete.
async fn rest_delete_handler<A>(
    State(rest): State<RestState<A>>,
    RestSecurityContext(security_ctx): RestSecurityContext,
    request: Request<Body>,
) -> Response
where
    A: DatabaseAdapter + SupportsMutations + Clone + Send + Sync + 'static,
{
    let (parts, _body) = request.into_parts();
    let relative_path = strip_base_path(&rest.route_table.base_path, parts.uri.path());
    let query_string = parts.uri.query().unwrap_or("");
    let query_pairs = parse_query_pairs(query_string);
    let query_refs: Vec<(&str, &str)> =
        query_pairs.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();

    let schema = rest.executor.schema();
    let config = schema.rest_config.as_ref().expect("REST config must exist: handler is only reached via a matched REST route, which requires rest_config to be present in the schema");
    let handler = RestHandler::new(&rest.executor, schema, config, &rest.route_table)
        .with_function_hooks(rest.function_hooks.as_ref());

    let result = handler
        .handle_delete(&relative_path, &query_refs, &parts.headers, security_ctx.as_ref())
        .await;

    rest_result_to_response(result, &rest.error_sanitizer)
}

/// Resolve a client's `Last-Event-ID` into the state a resumed stream reads from, or
/// the refusal owed instead (#1310).
///
/// The ladder, in the order a request meets it:
///
/// 1. **Not a resume** — a fresh connection, `Ok(None)`, and no read at all.
/// 2. **No reader** — this deployment keeps no record of what was delivered (its events do not come
///    from the local change log), so `501 RESUMPTION_UNSUPPORTED`, which is what this endpoint
///    answered for every resume before this issue.
/// 3. **Anchor unknown** — pruned, or an id from another stream: `410 RESUME_POINT_UNKNOWN`.
///    Refused rather than answered from the top of the log, which would skip silently.
/// 4. **Too far behind** — more delivered events since the anchor than
///    `[rest].sse_max_replay_events`: `413 RESUME_TOO_FAR_BEHIND`, before a single frame, because a
///    truncated replay is indistinguishable from a complete one.
/// 5. Otherwise the position to read forward from.
///
/// A database error resolving any of this is a refusal too, not a fresh stream: falling
/// back to "events from now on" is precisely the silent gap being refused.
#[cfg(feature = "observers")]
async fn resume_state<A>(
    rest: &RestState<A>,
    entity_type: &str,
    tenant: &fraiseql_observers::transport::TenantScope,
    resume: super::sse::ResumeRequest,
) -> Result<Option<super::resumable_stream::ResumeState>, super::handler::RestError>
where
    A: DatabaseAdapter + Clone + Send + Sync + 'static,
{
    use fraiseql_observers::listener::ResumeAnchor;

    let super::sse::ResumeRequest::From(seq) = resume else {
        return Ok(None);
    };

    let Some(reader) = rest.stream_replay.clone() else {
        return Err(super::sse::resumption_unsupported(seq));
    };

    let scope = super::sse::replay_scope(entity_type, tenant);

    let anchor = reader.anchor(&scope, seq).await.map_err(|error| {
        tracing::error!(%error, seq, "failed to resolve a stream resume point");
        super::handler::RestError::internal("Could not resolve the resume point")
    })?;
    let ResumeAnchor::Found(origin) = anchor else {
        return Err(super::sse::resume_point_unknown(seq));
    };

    // `0` is "no bound", and is the permissive setting: every resume is served however
    // far back it reaches.
    let cap = rest
        .executor
        .schema()
        .rest_config
        .as_ref()
        .map_or(super::sse::DEFAULT_SSE_MAX_REPLAY_EVENTS, |c| c.sse_max_replay_events);
    if cap > 0 {
        let backlog = reader.count_since(&origin, cap + 1).await.map_err(|error| {
            tracing::error!(%error, seq, "failed to measure a stream resume backlog");
            super::handler::RestError::internal("Could not measure the resume backlog")
        })?;
        let in_flight =
            reader.count_in_flight(&scope, &origin, cap + 1).await.map_err(|error| {
                tracing::error!(%error, seq, "failed to measure a stream in-flight tail");
                super::handler::RestError::internal("Could not measure the resume backlog")
            })?;
        if backlog > cap || in_flight > cap {
            return Err(super::sse::resume_too_far_behind(seq, cap));
        }
    }

    Ok(Some(super::resumable_stream::ResumeState {
        reader,
        scope,
        origin,
    }))
}

/// SSE handler — stream entity change events in real-time.
///
/// Returns `501 Not Implemented` when the `observers` feature is disabled.
/// Otherwise, streams events for the given resource type via SSE.
///
/// The context must be extracted whether or not the body reads it: running
/// [`RestSecurityContext`]'s extractor **is** the `require_auth` enforcement. Do not
/// replace it with a bare `_: RestSecurityContext` removal — that would restore the
/// pre-#810 state where this was the only route honouring the flag, inverted.
///
/// The body reads it as well, since #1113: it scopes the event subscription to the
/// caller's tenant. Before that it was bound as `_security_ctx` and discarded.
async fn rest_sse_handler<A>(
    State(rest): State<RestState<A>>,
    // Read only by the live-event branch below, which is `observers`-gated, so a
    // `rest`-without-`observers` build warns that it is unused — a warning preflight
    // cannot see, because it lints `--all-features` (the `server-rest` feature-matrix
    // combo is clippy-enabled and does see it).
    //
    // Silenced rather than renamed to `_security_ctx`: that underscore *was* #1113 — the
    // caller's context bound and discarded — and restoring the spelling would make the
    // defect look like the intended state again. Same treatment, and the same reason, as
    // `mount` in `derive_rest_context` (#1291).
    #[cfg_attr(not(feature = "observers"), allow(unused_variables))]
    RestSecurityContext(security_ctx): RestSecurityContext,
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
            return rest_result_to_response(
                Err(super::handler::RestError::not_found("Stream endpoint not found")),
                &rest.error_sanitizer,
            );
        },
    };

    // Verify the resource exists
    let schema = rest.executor.schema();
    let has_resource = rest.route_table.resources.iter().any(|r| r.name == resource_name);

    if !has_resource {
        return rest_result_to_response(
            Err(super::handler::RestError::not_found(format!(
                "Resource not found: {resource_name}"
            ))),
            &rest.error_sanitizer,
        );
    }

    // `require_auth` is enforced during context extraction by `RestSecurityContext`,
    // uniformly for every REST route. This handler used to carry the workspace's only
    // copy of that check (#810).

    // Read heartbeat interval from REST config (or use default).
    let heartbeat_secs = schema
        .rest_config
        .as_ref()
        .map_or(super::sse::DEFAULT_SSE_HEARTBEAT_SECONDS, |c| c.sse_heartbeat_seconds);

    // Check if observers feature is available
    #[cfg(not(feature = "observers"))]
    {
        let _ = heartbeat_secs; // suppress unused warning
        rest_result_to_response(Err(super::sse::observers_not_available()), &rest.error_sanitizer)
    }

    // With observers feature: set up SSE stream with real event subscription.
    #[cfg(feature = "observers")]
    {
        let heartbeat_interval = std::time::Duration::from_secs(heartbeat_secs);

        // The fan-out is `Some` exactly when an observer runtime is configured, so this
        // arm means "there is a producer", not merely "the field was populated".
        if let Some(ref fanout) = rest.event_fanout {
            use futures::StreamExt;

            // The refusals that need nothing from the database come first — a request
            // that cannot be served must not attach a receiver first. They sit *inside*
            // this arm because with no producer the whole truth about this endpoint is
            // the 501 below — a 403 there would imply that a tenant-bearing credential
            // would get a stream.
            //
            // #1113: `Last-Event-ID` used to be read into `_last_event_id` and dropped,
            // so a browser `EventSource` reconnecting after a blip silently lost the gap
            // while the transport reported a healthy stream. #1310 resumes it; a header
            // that is not an id this stream issues is still refused here.
            let resume = match super::sse::stream_resume_request(&parts.headers) {
                Ok(resume) => resume,
                Err(refusal) => {
                    return rest_result_to_response(Err(refusal), &rest.error_sanitizer);
                },
            };

            // Reading the header above is free; *resolving* it against the change log is
            // not, so the tenant gate sits between the two. An unauthorised caller is
            // turned away before anything is read on its behalf — and its anchor is
            // resolved within its own tenant or not at all, which is why the 410 for "no
            // such event here" cannot be used to probe another tenant's sequences.
            //
            // #1113: the subscription used to carry no tenant at all, and `tenant_id:
            // None` means *every tenant* — so one authenticated caller would have
            // received every tenant's events, `data` payload included.
            let tenant = match super::sse::stream_tenant_scope(
                security_ctx.as_ref(),
                schema.is_multi_tenant(),
            ) {
                Ok(scope) => scope,
                Err(refusal) => {
                    return rest_result_to_response(Err(refusal), &rest.error_sanitizer);
                },
            };

            // The GraphQL TYPE, not the route name. `/rest/v1/users/stream` streams
            // `User` events: the change log stamps `object_type` with the type name, and
            // so does everything downstream of it. The pre-#1309 code filtered on
            // `resource_name` — `users` — which matches no event ever written, so the
            // branch would have opened a connection and delivered nothing even once the
            // transport was populated. Unreachable code cannot be wrong in a way anyone
            // notices; this is what that looks like when it is finally reached.
            let Some(entity_type) = rest
                .route_table
                .resources
                .iter()
                .find(|r| r.name == resource_name)
                .map(|r| r.type_name.clone())
            else {
                return rest_result_to_response(
                    Err(super::handler::RestError::not_found(format!(
                        "Resource not found: {resource_name}"
                    ))),
                    &rest.error_sanitizer,
                );
            };

            // A broadcast receiver, so this stream is a fan-out consumer and not a
            // competing one: it takes nothing from the observer executor, which sits
            // upstream of the bridge that publishes here (#1309).
            //
            // Taken BEFORE the resume reads below, and that order is load-bearing: every
            // event published from this instant reaches this receiver, so the reads only
            // have to account for what came before it. Resolving the anchor first would
            // leave a window in which an event is published (missed by a receiver that
            // does not exist yet) and recorded (missed by a read that already ran) —
            // carried by neither path, on a stream that looks healthy.
            let receiver = fanout.subscribe();

            // #1310: resolve the client's resume point against the recorded delivery
            // order. Each refusal drops the receiver taken above, which costs nothing —
            // a broadcast sender does not wait on receivers.
            let resume = match resume_state(&rest, &entity_type, &tenant, resume).await {
                Ok(resume) => resume,
                Err(refusal) => {
                    return rest_result_to_response(Err(refusal), &rest.error_sanitizer);
                },
            };

            let entity_events = super::resumable_stream::resumable_event_stream(
                receiver,
                entity_type,
                tenant,
                resume,
            );

            // Merge entity events with heartbeat ticks.
            let heartbeat = futures::stream::unfold((), move |()| async move {
                tokio::time::sleep(heartbeat_interval).await;
                let event = axum::response::sse::Event::default().event("ping").data("");
                Some((event, ()))
            });

            let merged = futures::stream::select(entity_events, heartbeat)
                .map(Ok::<_, std::convert::Infallible>);

            let sse = axum::response::sse::Sse::new(merged).keep_alive(
                axum::response::sse::KeepAlive::new().interval(heartbeat_interval).text(""),
            );

            return axum::response::IntoResponse::into_response(sse);
        }

        // #873.4: no producer, so this endpoint cannot deliver an entity event. Say so,
        // with the same 501 the `#[cfg(not(feature = "observers"))]` arm returns.
        //
        // It used to answer 200 with a stream that emitted `event: ping` every
        // `sse_heartbeat_seconds` and nothing else — while the served OpenAPI described
        // it as "Subscribe to real-time changes … Events: insert, update, delete, ping".
        // A dashboard opening that stream sees a healthy connection, so its reconnect and
        // error handling never fire, and it shows stale data indefinitely. Enabling the
        // `observers` feature therefore turned an honest 501 into a silent no-op — the
        // feature flag made the server *less* truthful.
        //
        // Still reachable, and still the right answer: `event_fanout` is `Some` exactly
        // when an observer runtime is configured (`Server::entity_event_fanout`). A build
        // with the `observers` feature but no `[observers]` configuration has no producer
        // for the change log at all, so there is nothing to stream and a 200 here would
        // be the same lie in a new place.
        //
        // The branch above is no longer unreachable: #1309 wired it to the
        // `EventBridge`'s fan-out, which is a broadcast, rather than to
        // `EventTransport::subscribe`, which is a competing consumer on all three
        // transports and would have made an open browser tab take events away from the
        // observer executor.
        let _ = heartbeat_interval;
        rest_result_to_response(Err(super::sse::observers_not_available()), &rest.error_sanitizer)
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Read and parse a JSON request body.
// Reason: the `Err` variant IS the protocol's rejection value — it is returned to
// the client verbatim. Boxing it to shrink the `Result` would add an allocation
// on every rejection and force each `?` site to unbox what it is about to return.
#[allow(clippy::result_large_err)]
async fn read_json_body(body: Body) -> Result<serde_json::Value, Response> {
    let Ok(bytes) = axum::body::to_bytes(body, 1_048_576).await else {
        return Err(error_response(
            StatusCode::PAYLOAD_TOO_LARGE,
            "PAYLOAD_TOO_LARGE",
            "Request body too large",
        ));
    };

    if bytes.is_empty() {
        return Ok(serde_json::Value::Object(serde_json::Map::new()));
    }

    serde_json::from_slice(&bytes).map_err(|e| {
        error_response(StatusCode::BAD_REQUEST, "INVALID_JSON", &format!("Invalid JSON body: {e}"))
    })
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------
