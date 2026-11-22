//! HTTP handlers for observer management endpoints.

use axum::{
    Json,
    extract::{Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
};
use fraiseql_core::{
    runtime::Executor,
    schema::{CompiledSchema, ConditionRead},
};
use uuid::Uuid;

use super::{
    ActionConfig, CreateObserverRequest, ListObserverLogsQuery, ListObserversQuery,
    ObserverRepository, ObserverScope, OperatorOnly, PaginatedResponse, UpdateObserverRequest,
};
use crate::{ServerError, extractors::OptionalSecurityContext};

/// Refuse a webhook `body_template` that names a path the observed entity cannot fill.
///
/// The runtime renders a webhook template against the changed row and dead-letters an
/// event whose row has no value for a placeholder, so a typo surfaces only when the first
/// event fires. Where the compiled schema declares `entity_type`, each placeholder's
/// dotted path is resolved here against that type's fields
/// ([`CompiledSchema::document_path`], the rule the compiler applies to a declared
/// observer), and the first that cannot resolve is refused with `422`. A segment names a
/// field by the key the row holds for it, which is the key the renderer reads:
/// `review_status` for a field published as `reviewStatus`. An entity type the schema does
/// not declare, and a path that descends into JSON, a custom scalar, an interface or a
/// union, are left to delivery: the schema cannot say what they hold.
pub(super) fn reject_unfillable_templates(
    schema: &CompiledSchema,
    entity_type: Option<&str>,
    actions: &[ActionConfig],
) -> Option<Response> {
    let entity = schema.find_type(entity_type?)?;
    actions.iter().find_map(|action| {
        let ActionConfig::Webhook {
            body_template: Some(template),
            ..
        } = action
        else {
            return None;
        };
        let (path, unfillable) = fraiseql_observers::body_template_placeholders(template)
            .into_iter()
            .find_map(|path| schema.document_path(entity, &path).err().map(|why| (path, why)))?;
        let held = unfillable
            .held
            .map_or_else(String::new, |key| format!(" The row holds that field as `{key}`."));
        Some(
            (
                StatusCode::UNPROCESSABLE_ENTITY,
                Json(serde_json::json!({
                    "error": format!(
                        "Webhook body_template placeholder `{{{{ {path} }}}}` names no field of \
                         '{}': every event would be dead-lettered at delivery.{held}",
                        entity.name
                    )
                })),
            )
                .into_response(),
        )
    })
}

/// What to log of an observer the admin API accepts: each webhook `body_template`
/// placeholder a captured row of the entity cannot fill
/// ([`CompiledSchema::captured_row_warning`], the text the compiler prints for a declared
/// observer). The response does not carry it.
pub(super) fn captured_row_warnings(
    schema: &CompiledSchema,
    entity_type: Option<&str>,
    actions: &[ActionConfig],
) -> Vec<String> {
    let Some(entity) = entity_type.and_then(|name| schema.find_type(name)) else {
        return Vec::new();
    };
    actions
        .iter()
        .filter_map(|action| match action {
            ActionConfig::Webhook { body_template, .. } => body_template.as_deref(),
            _ => None,
        })
        .flat_map(fraiseql_observers::body_template_placeholders)
        .filter_map(|path| schema.captured_row_warning(entity, &path))
        .collect()
}

/// Check each key a condition reads against the observed entity
/// ([`CompiledSchema::condition_key`], the rule the compiler applies to a declared
/// observer): the warnings to log once the observer is stored, one per message.
///
/// An entity type the schema does not declare, and a condition that does not parse
/// (refused before this runs), are left alone.
///
/// # Errors
///
/// Returns the refusal of the first key no event of the entity can hold, which
/// [`condition_refused`] answers with `422`.
pub(super) fn check_condition_keys(
    schema: &CompiledSchema,
    entity_type: Option<&str>,
    condition: Option<&str>,
) -> Result<Vec<String>, String> {
    let (Some(entity), Some(condition)) =
        (entity_type.and_then(|n| schema.find_type(n)), condition)
    else {
        return Ok(Vec::new());
    };
    let Ok(ast) = fraiseql_observers::ConditionParser::new().parse(condition) else {
        return Ok(Vec::new());
    };
    let mut warnings: Vec<String> = Vec::new();
    for (key, read) in ast.field_references() {
        if let Some(warning) = schema.condition_key(entity, key, condition_read(read))? {
            if !warnings.contains(&warning) {
                warnings.push(warning);
            }
        }
    }
    Ok(warnings)
}

/// `422` for a condition [`check_condition_keys`] refuses.
fn condition_refused(refusal: &str) -> Response {
    (StatusCode::UNPROCESSABLE_ENTITY, Json(serde_json::json!({ "error": refusal })))
        .into_response()
}

/// The schema's name for how a condition reads a key.
const fn condition_read(read: fraiseql_observers::FieldRead) -> ConditionRead {
    use fraiseql_observers::FieldRead;
    match read {
        FieldRead::Value => ConditionRead::Value,
        FieldRead::Presence => ConditionRead::Presence,
        FieldRead::Change => ConditionRead::Change,
    }
}

/// Log [`captured_row_warnings`] for an observer about to be stored.
fn warn_captured_row_templates(
    schema: &CompiledSchema,
    entity_type: Option<&str>,
    actions: &[ActionConfig],
) {
    for message in captured_row_warnings(schema, entity_type, actions) {
        tracing::warn!(entity_type, "{message}");
    }
}

/// Application state for observer handlers.
#[derive(Clone)]
pub struct ObserverState {
    /// Repository used by all observer HTTP handlers.
    pub repository: ObserverRepository,
    /// Optional handle to the in-process observer runtime.
    ///
    /// When present, a successful create/update/delete/enable/disable write
    /// refreshes the runtime's `EventMatcher` so the admin API is
    /// self-consistent without a manual `POST /api/observers/runtime/reload`
    /// (#466). `None` when the admin API is mounted without a running
    /// background runtime (e.g. a control-plane-only deployment, or tests).
    pub runtime:    Option<std::sync::Arc<tokio::sync::RwLock<super::ObserverRuntime>>>,
    /// The executor serving queries, read on each write for the compiled schema a webhook
    /// template is checked against (`reject_unfillable_templates`). Reload-aware: a
    /// hot-reloaded schema is the one checked.
    pub executor:   std::sync::Arc<arc_swap::ArcSwap<Executor>>,
}

/// Refresh the in-process matcher after a successful observer write so the admin
/// API is self-consistent without a manual `…/runtime/reload` call (#466).
///
/// Best-effort: the write is already committed to `tb_observer`, so a reload
/// failure is logged rather than failing the request (which would invite a
/// duplicate write). The persisted change still goes live on the next
/// reload/poll/restart, and the manual reload endpoint remains as a retry.
async fn refresh_runtime_after_write(state: &ObserverState) {
    let Some(runtime) = state.runtime.as_ref() else {
        return;
    };
    match runtime.read().await.reload_observers().await {
        Ok(count) => {
            tracing::debug!(observer_count = count, "Observer matcher reloaded after admin write");
        },
        Err(e) => {
            tracing::warn!(
                error = %e,
                "Observer write persisted but the in-process matcher reload failed; the change \
                 will go live on the next reload/poll/restart. Retry with \
                 POST /api/observers/runtime/reload."
            );
        },
    }
}

/// The response for a repository failure.
///
/// A conflict or a validation failure carries a message this crate wrote, about the request.
/// Anything else is the database's own text, which names tables, columns and constraints: it is
/// logged and the caller gets a generic message.
fn repository_error(operation: &str, error: &ServerError) -> Response {
    let (status, message) = match error {
        ServerError::Conflict(message) => (StatusCode::CONFLICT, message.clone()),
        ServerError::Validation(message) => (StatusCode::BAD_REQUEST, message.clone()),
        other => {
            tracing::error!(error = %other, "Failed to {operation}");
            (StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to {operation}"))
        },
    };
    (status, Json(serde_json::json!({ "error": message }))).into_response()
}

fn observer_not_found() -> Response {
    (
        StatusCode::NOT_FOUND,
        Json(serde_json::json!({ "error": "Observer not found" })),
    )
        .into_response()
}

/// Reject an `event_type` the observer runtime does not emit.
fn reject_unknown_event_type(event_type: Option<&String>) -> Option<Response> {
    let valid_types = ["INSERT", "UPDATE", "DELETE", "CUSTOM"];
    event_type.filter(|t| !valid_types.contains(&t.as_str())).map(|event_type| {
        (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "error": format!("Invalid event_type '{}'. Must be one of: {:?}", event_type, valid_types)
            })),
        )
            .into_response()
    })
}

/// Refuse a `condition_expression` the observer runtime cannot parse.
///
/// The runtime parses a condition when an event reaches the observer, so one that does not
/// parse was stored and then failed on every event: the observer never ran. It is parsed
/// here with the runtime's own parser, as the compiler parses a condition declared in the
/// schema, and refused with `400` naming the field and the parser's reason. An empty
/// condition does not parse either: to create an observer with no condition, leave the
/// field out; to clear one, send it as `null`.
fn reject_unparsable_condition(condition: Option<&str>) -> Option<Response> {
    let reason = fraiseql_observers::ConditionParser::new().parse(condition?).err()?;
    Some(
        (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "error": format!("condition_expression does not parse: {reason}")
            })),
        )
            .into_response(),
    )
}

/// List the observers the caller can see.
///
/// GET /api/observers
pub async fn list_observers(
    State(state): State<ObserverState>,
    scope: ObserverScope,
    Query(query): Query<ListObserversQuery>,
) -> Response {
    match state.repository.list(&query, scope).await {
        Ok((observers, total_count)) => {
            // R8: never echo webhook secrets (actions[].headers) in a response body.
            let observers: Vec<_> =
                observers.into_iter().map(super::Observer::with_redacted_secrets).collect();
            let response =
                PaginatedResponse::new(observers, query.page, query.page_size, total_count);
            (StatusCode::OK, Json(response)).into_response()
        },
        Err(e) => repository_error("list observers", &e),
    }
}

/// Get a single observer the caller can see.
///
/// GET /api/observers/{id}
pub async fn get_observer(
    State(state): State<ObserverState>,
    scope: ObserverScope,
    Path(id): Path<Uuid>,
) -> Response {
    match state.repository.get_by_id(id, scope).await {
        Ok(Some(observer)) => {
            (StatusCode::OK, Json(observer.with_redacted_secrets())).into_response()
        },
        Ok(None) => observer_not_found(),
        Err(e) => repository_error("get observer", &e),
    }
}

/// Create a new observer, owned by the caller's organisation (global for the operator).
///
/// POST /api/observers
pub async fn create_observer(
    State(state): State<ObserverState>,
    scope: ObserverScope,
    OptionalSecurityContext(security_context): OptionalSecurityContext,
    Json(request): Json<CreateObserverRequest>,
) -> Response {
    if request.name.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": "Name is required" })),
        )
            .into_response();
    }

    if request.actions.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": "At least one action is required" })),
        )
            .into_response();
    }

    // Reject action types the runtime cannot dispatch (#612 item 10) — 400 now
    // rather than 201-then-silently-skipped at load.
    if let Some(resp) = reject_unknown_event_type(request.event_type.as_ref()) {
        return resp;
    }
    if let Some(resp) = reject_unparsable_condition(request.condition_expression.as_deref()) {
        return resp;
    }
    let executor = state.executor.load();
    let condition_warnings = match check_condition_keys(
        executor.schema(),
        request.entity_type.as_deref(),
        request.condition_expression.as_deref(),
    ) {
        Ok(warnings) => warnings,
        Err(refusal) => return condition_refused(&refusal),
    };
    if let Some(resp) = reject_unfillable_templates(
        executor.schema(),
        request.entity_type.as_deref(),
        &request.actions,
    ) {
        return resp;
    }
    for message in condition_warnings {
        tracing::warn!(entity_type = request.entity_type.as_deref(), "{message}");
    }
    warn_captured_row_templates(
        executor.schema(),
        request.entity_type.as_deref(),
        &request.actions,
    );
    drop(executor);

    let created_by = security_context.as_ref().map(|ctx| ctx.user_id.as_str());

    match state.repository.create(&request, scope, created_by).await {
        Ok(observer) => {
            refresh_runtime_after_write(&state).await;
            (StatusCode::CREATED, Json(observer.with_redacted_secrets())).into_response()
        },
        Err(e) => repository_error("create observer", &e),
    }
}

/// Update an observer the caller owns.
///
/// PATCH /api/observers/{id}
pub async fn update_observer(
    State(state): State<ObserverState>,
    scope: ObserverScope,
    OptionalSecurityContext(security_context): OptionalSecurityContext,
    Path(id): Path<Uuid>,
    Json(request): Json<UpdateObserverRequest>,
) -> Response {
    // Before the stored observer is read: a condition that does not parse is refused
    // whatever the id names.
    if let Some(resp) = reject_unparsable_condition(
        request.condition_expression.as_ref().and_then(Option::as_deref),
    ) {
        return resp;
    }
    if let Some(resp) = reject_unknown_event_type(request.event_type.as_ref()) {
        return resp;
    }
    // Reject action types the runtime cannot dispatch (#612 item 10).
    if let Some(resp) = check_updated_observer(&state, id, &request, scope).await {
        return resp;
    }

    let updated_by = security_context.as_ref().map(|ctx| ctx.user_id.as_str());
    write_observer(&state, id, &request, scope, updated_by, "update observer").await
}

/// The observer an update leaves, checked as a create is: its webhook templates
/// (`reject_unfillable_templates`) when the entity type or the actions change, and its
/// condition (`check_condition_keys`) when the entity type or the condition changes. The
/// unchanged parts are read from the stored observer. An update that sends only actions
/// does not answer for the stored condition, which was checked on its own write.
async fn check_updated_observer(
    state: &ObserverState,
    id: Uuid,
    request: &UpdateObserverRequest,
    scope: ObserverScope,
) -> Option<Response> {
    let check_templates = request.entity_type.is_some() || request.actions.is_some();
    let check_condition = request.entity_type.is_some() || request.condition_expression.is_some();
    if !check_templates && !check_condition {
        return None;
    }
    let complete = request.entity_type.is_some()
        && request.actions.is_some()
        && request.condition_expression.is_some();
    let stored = if complete {
        None
    } else {
        match state.repository.get_by_id(id, scope).await {
            Ok(Some(observer)) => Some(observer),
            Ok(None) => return Some(observer_not_found()),
            Err(e) => return Some(repository_error("update observer", &e)),
        }
    };
    let entity_type = request
        .entity_type
        .clone()
        .or_else(|| stored.as_ref().and_then(|o| o.entity_type.clone()));
    let condition = match &request.condition_expression {
        Some(condition) => condition.clone(),
        None => stored.as_ref().and_then(|o| o.condition_expression.clone()),
    };
    let actions = match &request.actions {
        Some(actions) => actions.clone(),
        // Stored actions were validated on their own write; ones that no longer parse as
        // this DTO are left to the runtime's load.
        None => stored
            .and_then(|o| serde_json::from_value::<Vec<ActionConfig>>(o.actions).ok())
            .unwrap_or_default(),
    };
    let executor = state.executor.load();
    let schema = executor.schema();
    let condition_warnings = if check_condition {
        match check_condition_keys(schema, entity_type.as_deref(), condition.as_deref()) {
            Ok(warnings) => warnings,
            Err(refusal) => return Some(condition_refused(&refusal)),
        }
    } else {
        Vec::new()
    };
    if check_templates {
        if let Some(refused) = reject_unfillable_templates(schema, entity_type.as_deref(), &actions)
        {
            return Some(refused);
        }
    }
    for message in condition_warnings {
        tracing::warn!(entity_type = entity_type.as_deref(), "{message}");
    }
    if check_templates {
        warn_captured_row_templates(schema, entity_type.as_deref(), &actions);
    }
    None
}

/// Delete an observer the caller owns (soft delete).
///
/// DELETE /api/observers/{id}
pub async fn delete_observer(
    State(state): State<ObserverState>,
    scope: ObserverScope,
    Path(id): Path<Uuid>,
) -> Response {
    match state.repository.delete(id, scope).await {
        Ok(true) => {
            refresh_runtime_after_write(&state).await;
            StatusCode::NO_CONTENT.into_response()
        },
        Ok(false) => observer_not_found(),
        Err(e) => repository_error("delete observer", &e),
    }
}

/// Get statistics for the observers the caller owns.
///
/// GET /api/observers/stats
/// GET /api/observers/{id}/stats
pub async fn get_observer_stats(
    State(state): State<ObserverState>,
    scope: ObserverScope,
    observer_id: Option<Path<Uuid>>,
) -> Response {
    match state.repository.get_stats(observer_id.map(|p| p.0), scope).await {
        Ok(stats) => (StatusCode::OK, Json(stats)).into_response(),
        Err(e) => repository_error("get observer stats", &e),
    }
}

/// List execution logs of the observers the caller owns.
///
/// GET /api/observers/logs
/// GET /api/observers/{id}/logs
pub async fn list_observer_logs(
    State(state): State<ObserverState>,
    scope: ObserverScope,
    Path(observer_id): Path<Option<Uuid>>,
    Query(mut query): Query<ListObserverLogsQuery>,
) -> Response {
    if let Some(id) = observer_id {
        query.observer_id = Some(id);
    }

    match state.repository.list_logs(&query, scope).await {
        Ok((logs, total_count)) => {
            let response = PaginatedResponse::new(logs, query.page, query.page_size, total_count);
            (StatusCode::OK, Json(response)).into_response()
        },
        Err(e) => repository_error("list observer logs", &e),
    }
}

/// Enable an observer the caller owns.
///
/// POST /api/observers/{id}/enable
pub async fn enable_observer(
    State(state): State<ObserverState>,
    scope: ObserverScope,
    OptionalSecurityContext(security_context): OptionalSecurityContext,
    Path(id): Path<Uuid>,
) -> Response {
    let request = UpdateObserverRequest {
        enabled: Some(true),
        ..Default::default()
    };
    let updated_by = security_context.as_ref().map(|ctx| ctx.user_id.as_str());
    write_observer(&state, id, &request, scope, updated_by, "enable observer").await
}

/// Disable an observer the caller owns.
///
/// POST /api/observers/{id}/disable
pub async fn disable_observer(
    State(state): State<ObserverState>,
    scope: ObserverScope,
    OptionalSecurityContext(security_context): OptionalSecurityContext,
    Path(id): Path<Uuid>,
) -> Response {
    let request = UpdateObserverRequest {
        enabled: Some(false),
        ..Default::default()
    };
    let updated_by = security_context.as_ref().map(|ctx| ctx.user_id.as_str());
    write_observer(&state, id, &request, scope, updated_by, "disable observer").await
}

async fn write_observer(
    state: &ObserverState,
    id: Uuid,
    request: &UpdateObserverRequest,
    scope: ObserverScope,
    updated_by: Option<&str>,
    operation: &str,
) -> Response {
    match state.repository.update(id, request, scope, updated_by).await {
        Ok(Some(observer)) => {
            refresh_runtime_after_write(state).await;
            (StatusCode::OK, Json(observer.with_redacted_secrets())).into_response()
        },
        Ok(None) => observer_not_found(),
        Err(e) => repository_error(operation, &e),
    }
}

// ============================================================================
// Runtime Health Check Handlers
// ============================================================================

use std::sync::Arc;

use tokio::sync::RwLock;

/// State for runtime health checks
#[derive(Clone)]
pub struct RuntimeHealthState {
    /// Reference to the observer runtime (wrapped in `RwLock` for thread safety)
    pub runtime: Arc<RwLock<super::ObserverRuntime>>,
}

/// Get observer runtime health status.
///
/// GET /api/observers/runtime/health
pub async fn get_runtime_health(
    State(state): State<RuntimeHealthState>,
    _: OperatorOnly,
) -> impl IntoResponse {
    let runtime = state.runtime.read().await;
    let health = runtime.health();

    let status = if health.running {
        StatusCode::OK
    } else {
        StatusCode::SERVICE_UNAVAILABLE
    };

    // Convert RuntimeHealth to JSON-serializable format
    let health_json = serde_json::json!({
        "running": health.running,
        "observer_count": health.observer_count,
        "last_checkpoint": health.last_checkpoint,
        "events_processed": health.events_processed,
        "errors": health.errors
    });

    (status, Json(health_json)).into_response()
}

/// Reload observers from database.
///
/// POST /api/observers/runtime/reload
pub async fn reload_observers(
    State(state): State<RuntimeHealthState>,
    _: OperatorOnly,
) -> impl IntoResponse {
    let runtime = state.runtime.read().await;

    match runtime.reload_observers().await {
        Ok(count) => (
            StatusCode::OK,
            Json(serde_json::json!({
                "message": "Observers reloaded successfully",
                "count": count
            })),
        )
            .into_response(),
        Err(e) => repository_error("reload observers", &e),
    }
}
