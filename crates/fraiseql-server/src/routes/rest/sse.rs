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

/// Default bound on how far back a `Last-Event-ID` resume may reach, used when no
/// compiled `[rest]` config is present.
///
/// Mirrors `RestConfig::default().sse_max_replay_events`; a schema that carries a
/// `rest_config` supplies its own.
pub const DEFAULT_SSE_MAX_REPLAY_EVENTS: u64 = 10_000;

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
// Live-branch decisions (#1113, #1309)
//
// The decisions the live-event branch of `rest_sse_handler` makes are here, as
// functions over their inputs. They were extracted because the branch was
// unreachable — `RestState.event_transport` was `None` at its only construction site
// and there was no setter — so what could be tested was pulled out of it rather than
// left as untested lines inside it.
//
// #1309 wired the branch, so it is now reachable and covered end to end by
// `rest_stream_fanout_e2e_pg`. These stay extracted anyway: an integration test can
// show that a tenant-scoped stream delivers its own tenant's events, but enumerating
// every arm of the tenant gate — including the fail-closed ones that deliver nothing —
// is what these unit tests do, and a test that asserts nothing arrives is far weaker
// over a socket than over a function.
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

/// What a request's `Last-Event-ID` asks for.
///
/// #1113 read this header into `let _last_event_id = …` and dropped it, so a browser
/// `EventSource` reconnecting after a blip silently lost every event in the gap while
/// the transport reported a healthy stream. #1310 makes it a resume.
#[cfg(feature = "observers")]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResumeRequest {
    /// No usable header: deliver from now on.
    Fresh,
    /// Resume after the event carrying this Change-Spine sequence.
    From(i64),
}

/// Read the `Last-Event-ID` header as a resume point.
///
/// The id a stream emits is the Change-Spine `seq` and nothing else (#1113 chose it over
/// the event UUID precisely so that this day could come), so a value that is not one is
/// not a resume point this stream ever issued. It is refused rather than ignored: a
/// client sending an id from another system, or a hand-typed one, is asking for
/// something the server cannot give, and starting from now instead would hand it the
/// silent gap in a new wrapper.
///
/// An absent or blank header is a fresh delivery, not a resume.
///
/// # Errors
///
/// Returns `400 RESUME_POINT_INVALID` when the header is present and not an integer.
#[cfg(feature = "observers")]
pub fn stream_resume_request(headers: &HeaderMap) -> Result<ResumeRequest, RestError> {
    let Some(raw) = extract_last_event_id(headers) else {
        return Ok(ResumeRequest::Fresh);
    };
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Ok(ResumeRequest::Fresh);
    }

    trimmed.parse::<i64>().map(ResumeRequest::From).map_err(|_| RestError {
        status:  StatusCode::BAD_REQUEST,
        code:    "RESUME_POINT_INVALID",
        message: format!(
            "Last-Event-ID {raw:?} is not an event id this stream issues. Every event \
             carries `id: <seq>`, the Change-Spine sequence of the change, so a resume \
             point is an integer. Reconnect without the header to receive events from \
             now on."
        ),
        details: None,
    })
}

/// The refusal owed to a client that asked to resume where nothing records what was
/// delivered.
///
/// Resuming reads back the observer runtime's dispatch ledger, which is written by the
/// PostgreSQL change-log poller. A deployment whose events reach the runtime by another
/// route has no such record — so there is no delivery order to resume from, and
/// answering `200` from the change log would serve rows this stream may never have
/// carried.
#[cfg(feature = "observers")]
#[must_use]
pub fn resumption_unsupported(seq: i64) -> RestError {
    RestError {
        status:  StatusCode::NOT_IMPLEMENTED,
        code:    "RESUMPTION_UNSUPPORTED",
        message: format!(
            "Last-Event-ID {seq} cannot be honoured: this deployment keeps no record of \
             what this stream delivered, so the events since {seq} cannot be \
             established. Reconnect without the header to receive events from now on."
        ),
        details: None,
    }
}

/// The refusal owed to a client whose resume point is no longer in the change log.
///
/// The named event has been pruned by retention, or was never on this stream at all (an
/// id picked up from another resource, tenant or deployment). Both are refused for one
/// reason: with the anchor gone, nothing can establish what came after it, and a `200`
/// carrying a partial replay would be the same silent gap in a new place.
#[cfg(feature = "observers")]
#[must_use]
pub fn resume_point_unknown(seq: i64) -> RestError {
    RestError {
        status:  StatusCode::GONE,
        code:    "RESUME_POINT_UNKNOWN",
        message: format!(
            "Last-Event-ID {seq} names no event on this stream: it has aged out of the \
             change log, or it was issued by a different stream. What followed it \
             cannot be established, so it is refused rather than answered with a replay \
             that might skip. Reconnect without the header to receive events from now on."
        ),
        details: None,
    }
}

/// The refusal owed to a client that is further behind than the deployment will replay.
///
/// Refused **before** the first frame rather than truncated after several thousand: a
/// client that receives part of its replay and then a live stream cannot tell that from
/// a complete one, which is the failure this endpoint exists to stop. The bound is
/// `[rest].sse_max_replay_events`.
#[cfg(feature = "observers")]
#[must_use]
pub fn resume_too_far_behind(seq: i64, cap: u64) -> RestError {
    RestError {
        status:  StatusCode::PAYLOAD_TOO_LARGE,
        code:    "RESUME_TOO_FAR_BEHIND",
        message: format!(
            "Last-Event-ID {seq} is more than {cap} delivered events behind, which is \
             this deployment's replay bound (`[rest].sse_max_replay_events`). Refusing \
             rather than replaying part of the gap. Reconnect without the header to \
             receive events from now on, or raise the bound."
        ),
        details: None,
    }
}

/// The change-log scope a resumed stream reads back, from the gates the live stream
/// applies.
///
/// Built from the live decisions rather than re-derived, so a replay cannot carry what
/// the live stream would have filtered: the entity type is the same GraphQL type name,
/// and the tenant is the same string [`stream_event_matches`] compares.
#[cfg(feature = "observers")]
#[must_use]
pub fn replay_scope(
    entity_type: &str,
    scope: &fraiseql_observers::transport::TenantScope,
) -> fraiseql_observers::listener::ReplayScope {
    use fraiseql_observers::transport::TenantScope;

    fraiseql_observers::listener::ReplayScope {
        object_type: entity_type.to_string(),
        tenant:      match scope {
            TenantScope::AllTenants => None,
            TenantScope::Tenant(tenant) => Some(tenant.clone()),
        },
    }
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
    /// Render a fanned-out entity event as the SSE frame it becomes.
    ///
    /// The id is the Change-Spine sequence and not the event's UUID. #1113: a UUID cannot be
    /// resolved to a resume point at all, so emitting one as the SSE id promises a client a
    /// resumption no implementation could ever provide. `seq` identifies the row a resume
    /// anchors on (#1310), so a client's stored id is the one a replay can resolve.
    ///
    /// ⚠ It is an **identifier**, not a watermark. `seq` is allocated when the writing
    /// transaction inserts and becomes visible when it commits, so a row that commits late is
    /// delivered *after* a higher sequence: the id a client holds is the last event it
    /// received, never the highest. Reading a resume back as `seq > <id>` therefore skips the
    /// straggler for ever — see [`fraiseql_observers::listener::replay`], which resumes from
    /// the recorded delivery order instead.
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
    pub fn from_bridge_event(event: &'a crate::subscriptions::EntityEvent) -> Self {
        Self {
            event_type: event_kind_to_sse_type(operation_kind(event.operation)),
            // `seq` rides the Change-Spine envelope on this side of the pipeline
            // (`process_entity_event` copies it there); the observer event's own `seq`
            // field does not survive the forward. An event whose producer stamped no
            // envelope field at all carries `change_spine: None`, which is the same
            // "no sequence" case as `seq: None` and takes the same answer: no `id:`.
            id:         event.change_spine.as_ref().and_then(|env| env.seq).map(|s| s.to_string()),
            data:       &event.data,
        }
    }
}

/// The change-log `event_type` spelling for a subscriber-visible operation.
///
/// Exists so [`event_kind_to_sse_type`] stays the one table mapping a kind to a wire
/// name, rather than this conversion growing a second copy of it.
///
/// ⚠ [`SubscriptionOperation`] is `#[non_exhaustive]`, so this match needs a wildcard
/// and a new variant added upstream will land in it rather than failing to compile.
/// It returns a spelling [`event_kind_to_sse_type`] does not know, so the frame goes out
/// as `event: unknown`, and it warns — rather than being silently folded into `insert`: a
/// new CDC operation delivered under the wrong name is worse than one delivered under
/// a name the client does not recognise. Grep here when adding a variant.
///
/// [`SubscriptionOperation`]: fraiseql_core::runtime::subscription::SubscriptionOperation
#[cfg(feature = "observers")]
#[must_use]
pub fn operation_kind(
    operation: fraiseql_core::runtime::subscription::SubscriptionOperation,
) -> &'static str {
    use fraiseql_core::runtime::subscription::SubscriptionOperation as Op;

    match operation {
        Op::Create => "INSERT",
        Op::Update => "UPDATE",
        Op::Delete => "DELETE",
        other => {
            tracing::warn!(
                operation = %other,
                "REST /{{resource}}/stream received a subscription operation it has no SSE \
                 event name for; delivering it as `event: unknown`. Add it to \
                 `routes::rest::sse::operation_kind`."
            );
            "UNRECOGNISED"
        },
    }
}

/// The SSE `event:` name for the one frame a lagging stream receives.
///
/// `error` rather than a name of its own: a browser `EventSource` fires its `error`
/// handler for a named `error` event, so a client that handles nothing else still sees
/// this one.
#[cfg(feature = "observers")]
pub const STREAM_LAGGED_EVENT: &str = "error";

/// The payload of the one frame a lagging stream receives before it ends.
///
/// A `broadcast` receiver that falls the channel's whole capacity behind is told how
/// many events it missed, and `recv` then **resumes from the oldest event still
/// buffered**. Resuming is the wrong answer here and the reason this frame exists: the
/// client cannot see the gap, so a stream that quietly carried on would look exactly
/// like one that never missed anything — the "healthy connection, stale data" failure
/// this endpoint has now been corrected for twice (#873.4, #1113).
///
/// So the stream emits this and **ends**. An ended stream makes `EventSource`
/// reconnect, which is visible; and the count says how much was lost, which a bare
/// disconnect would not.
#[cfg(feature = "observers")]
#[must_use]
pub fn stream_lagged_payload(skipped: u64) -> serde_json::Value {
    serde_json::json!({
        "code": "STREAM_LAGGED",
        "skipped": skipped,
        "message": format!(
            "This stream fell behind and {skipped} event(s) were dropped before they \
             could be delivered. It is ending rather than resuming past the gap, which \
             you could not have seen. Reconnect to receive events from now on."
        ),
    })
}

/// Whether one fanned-out entity event belongs on this resource's stream.
///
/// Two gates, and the tenant one is the security-relevant half (#1113):
///
/// - **entity type** — matched against the resource's `type_name` (`User`), never its route name
///   (`users`). The change log stamps `object_type` with the GraphQL type, which is also what
///   `SubscriptionManager` matches `definition.return_type` against, so the two streaming surfaces
///   agree on what an entity is called.
/// - **tenant** — [`TenantScope::Tenant`] matches only an event stamped with that exact tenant. An
///   event carrying **no** tenant does not match: a missing stamp is not a wildcard. Same
///   fail-closed rule as `SubscriptionManager`'s gate, so a multi-tenant deployment cannot leak an
///   untagged event to a tenant-scoped caller.
///
/// [`TenantScope::Tenant`]: fraiseql_observers::transport::TenantScope::Tenant
#[cfg(feature = "observers")]
#[must_use]
pub fn stream_event_matches(
    event: &crate::subscriptions::EntityEvent,
    entity_type: &str,
    scope: &fraiseql_observers::transport::TenantScope,
) -> bool {
    use fraiseql_observers::transport::TenantScope;

    if event.entity_type != entity_type {
        return false;
    }

    match scope {
        TenantScope::AllTenants => true,
        TenantScope::Tenant(tenant) => event.tenant_id.as_deref() == Some(tenant.as_str()),
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------
