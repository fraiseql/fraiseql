//! Server-Sent Events (SSE) handler for the REST transport.
//!
//! Provides real-time streaming of entity change events via SSE.
//! Requires the `observers` feature flag.
//!
//! ## Endpoints
//!
//! `GET /rest/v1/{resource}/stream` with `Accept: text/event-stream`
//!
//! ## Event Format
//!
//! ```text
//! event: insert
//! id: <event-uuid>
//! data: {"id":1,"name":"Alice"}
//!
//! event: update
//! id: <event-uuid>
//! data: {"id":1,"name":"Alice Updated"}
//!
//! event: delete
//! id: <event-uuid>
//! data: {"entity_id":"<uuid>"}
//! ```

use axum::http::{HeaderMap, StatusCode};

use super::handler::RestError;

/// Content type for SSE responses.
pub const SSE_CONTENT_TYPE: &str = "text/event-stream";

/// Default heartbeat interval in seconds.
pub const DEFAULT_SSE_HEARTBEAT_SECONDS: u64 = 30;

/// Check whether an `Accept` header value requests SSE.
#[must_use]
pub fn accepts_sse(headers: &HeaderMap) -> bool {
    headers.get("accept").and_then(|v| v.to_str().ok()).is_some_and(|accept| {
        accept.split(',').any(|part| part.trim().eq_ignore_ascii_case(SSE_CONTENT_TYPE))
    })
}

/// Check if a path ends with `/stream` (SSE route pattern).
#[must_use]
pub fn is_stream_path(relative_path: &str) -> bool {
    let segments: Vec<&str> = relative_path
        .trim_start_matches('/')
        .split('/')
        .filter(|s| !s.is_empty())
        .collect();
    segments.last().is_some_and(|s| *s == "stream")
}

/// Extract the resource name from a `/stream` path.
///
/// Given `/users/stream`, returns `Some("users")`.
/// Given `/users`, returns `None`.
#[must_use]
pub fn extract_stream_resource(relative_path: &str) -> Option<&str> {
    let segments: Vec<&str> = relative_path
        .trim_start_matches('/')
        .split('/')
        .filter(|s| !s.is_empty())
        .collect();

    if segments.len() == 2 && segments[1] == "stream" {
        Some(segments[0])
    } else {
        None
    }
}

/// Extract the `Last-Event-ID` header value for SSE reconnection.
#[must_use]
pub fn extract_last_event_id(headers: &HeaderMap) -> Option<String> {
    headers.get("last-event-id").and_then(|v| v.to_str().ok()).map(String::from)
}

/// Format an SSE event as a string.
///
/// Produces the wire format:
/// ```text
/// event: <event_type>
/// id: <event_id>
/// data: <json_data>
/// ```
#[must_use]
pub fn format_sse_event(event_type: &str, event_id: &str, data: &serde_json::Value) -> String {
    let data_str = serde_json::to_string(data).unwrap_or_default();
    format!("event: {event_type}\nid: {event_id}\ndata: {data_str}\n\n")
}

/// Format a heartbeat SSE event.
#[must_use]
pub fn format_heartbeat() -> String {
    "event: ping\ndata: \n\n".to_string()
}

/// Build the SSE "not implemented" error when the `observers` feature is disabled.
#[must_use]
pub fn observers_not_available() -> RestError {
    RestError {
        status:  StatusCode::NOT_IMPLEMENTED,
        code:    "NOT_IMPLEMENTED",
        message: "SSE streaming requires the observers feature".to_string(),
        details: None,
    }
}

/// Map an observer `EventKind` to the SSE event type string.
#[must_use]
pub fn event_kind_to_sse_type(kind: &str) -> &str {
    match kind {
        "INSERT" => "insert",
        "UPDATE" => "update",
        "DELETE" => "delete",
        "CUSTOM" => "custom",
        _ => "unknown",
    }
}

// ---------------------------------------------------------------------------
// Live-branch decisions (#1113)
//
// The three decisions the live-event branch of `rest_sse_handler` makes are here,
// as functions over their inputs, because the branch itself is unreachable:
// `RestState.event_transport` is `None` at its only construction site and there is
// no setter (#1309). Nothing can drive that branch end to end, so what *can* be
// tested is pulled out of it rather than left as untested lines inside it.
// ---------------------------------------------------------------------------

/// Which tenants' events a `/stream` subscription may receive, or the refusal to
/// return in place of a stream.
///
/// #1113: the handler extracted the caller's `SecurityContext` and discarded it,
/// building `EventFilter { entity_type, ..Default::default() }` — and `tenant_id:
/// None` meant *every tenant*. An authenticated caller on any tenant would have
/// received every tenant's change events, full `data` payload included, the moment
/// the transport was populated. Authentication is not authorisation: the REST read
/// surface's tenant scoping (#812/#739) lives in the query path, and a stream that
/// subscribes with no tenant bypasses it by construction.
///
/// The rule is the one the GraphQL subscription gate already applies
/// (`SubscriptionManager::event_matches`), keyed on the same
/// `CompiledSchema::is_multi_tenant`, so the two streaming surfaces cannot drift:
///
/// - **multi-tenant**: the principal's tenant scopes the subscription. A principal carrying no
///   tenant — including an absent principal, where `require_auth` is off — is **refused**.
///   Fail-closed: there is no tenant to scope by, and a deployment that declared itself
///   multi-tenant has said that matters.
/// - **single-tenant**: unscoped. Tenant ids are typically absent throughout such a deployment, so
///   scoping by an absent tenant would match nothing.
///
/// Refusing where `SubscriptionManager` merely delivers nothing is deliberate: an
/// SSE connection that opens and stays silent is the "looks healthy, is stale"
/// failure #873.4 removed from this very endpoint.
///
/// # Errors
///
/// Returns `403 TENANT_SCOPE_REQUIRED` when the deployment is multi-tenant and the
/// request carries no tenant to scope by.
#[cfg(feature = "observers")]
pub fn stream_tenant_scope(
    security_ctx: Option<&fraiseql_core::security::SecurityContext>,
    multi_tenant: bool,
) -> Result<fraiseql_observers::transport::TenantScope, RestError> {
    use fraiseql_observers::transport::TenantScope;

    if !multi_tenant {
        return Ok(TenantScope::AllTenants);
    }

    security_ctx.and_then(|ctx| ctx.tenant_id.as_ref()).map_or_else(
        || {
            Err(RestError {
                status:  StatusCode::FORBIDDEN,
                code:    "TENANT_SCOPE_REQUIRED",
                message: "This deployment is multi-tenant and the request carries no tenant, \
                          so an event stream cannot be scoped to one. Present a credential \
                          carrying a tenant."
                    .to_string(),
                details: None,
            })
        },
        |tenant| Ok(TenantScope::Tenant(tenant.as_str().to_string())),
    )
}

/// The refusal owed to a client that asked to resume, when resuming is not
/// implemented.
///
/// #1113: the handler read the header into `let _last_event_id = …` and dropped it.
/// A browser `EventSource` re-sends `Last-Event-ID` automatically on every
/// reconnect, so a client reconnecting after a network blip silently lost every
/// event in the gap while the transport reported a healthy stream.
///
/// Returns `Some(refusal)` when the client asked to resume. It is refused rather
/// than ignored: honouring it needs a durable `seq`-ranged read of
/// `core.tb_entity_change_log` (#1310), and the alternative — answering `200` and
/// starting from now — is the silent data loss this is here to stop. A reconnect
/// that fails loudly is diagnosable; one that succeeds while skipping a range is
/// not.
///
/// An absent or empty header is a fresh delivery, not a resume, and returns `None`.
#[cfg(feature = "observers")]
#[must_use]
pub fn stream_resume_refusal(headers: &HeaderMap) -> Option<RestError> {
    let raw = extract_last_event_id(headers)?;
    if raw.trim().is_empty() {
        return None;
    }

    Some(RestError {
        status:  StatusCode::NOT_IMPLEMENTED,
        code:    "RESUMPTION_UNSUPPORTED",
        message: format!(
            "Last-Event-ID {raw:?} cannot be honoured: this stream has no replay path yet, \
             and answering without one would skip every event since {raw:?} while looking \
             healthy. Reconnect without the header to receive events from now on."
        ),
        details: None,
    })
}

/// One entity event as it goes on the wire.
///
/// Borrows its payload: this is the shape of an SSE frame, not a copy of the event.
#[cfg(feature = "observers")]
#[derive(Debug, PartialEq, Eq)]
pub struct StreamEvent<'a> {
    /// The SSE `event:` field.
    pub event_type: &'static str,
    /// The SSE `id:` field, **absent** when the source row carried no sequence.
    pub id:         Option<String>,
    /// The SSE `data:` payload.
    pub data:       &'a serde_json::Value,
}

#[cfg(feature = "observers")]
impl<'a> StreamEvent<'a> {
    /// Render an entity event as the SSE frame it becomes.
    ///
    /// The id is [`EntityEvent::seq`] — the monotonic Change-Spine sequence — and not
    /// [`EntityEvent::id`], which is a UUID. #1113: a UUID cannot be resolved to a
    /// resume point by ordering, so emitting one as the SSE id promises a client a
    /// resumption that no implementation could ever provide. `seq` is what a replay
    /// would read by (#1310), so a client's stored id is already the right one on the
    /// day replay lands.
    ///
    /// `seq` is `Option<i64>` ("None when the source row carried no sequence"), and
    /// such an event carries **no `id:` field at all**. Per the SSE specification an
    /// absent `id` leaves the client's last-event-id buffer unchanged, so a reconnect
    /// still names the last event that *had* a sequence: at-least-once, never a skip.
    /// Emitting the UUID here instead would poison the buffer with a value no replay
    /// can resolve.
    ///
    /// [`EntityEvent::seq`]: fraiseql_observers::event::EntityEvent::seq
    /// [`EntityEvent::id`]: fraiseql_observers::event::EntityEvent::id
    #[must_use]
    pub fn from_entity_event(event: &'a fraiseql_observers::event::EntityEvent) -> Self {
        Self {
            event_type: event_kind_to_sse_type(event.event_type.as_str()),
            id:         event.seq.map(|seq| seq.to_string()),
            data:       &event.data,
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------
