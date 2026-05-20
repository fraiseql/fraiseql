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
        status: StatusCode::NOT_IMPLEMENTED,
        code: "NOT_IMPLEMENTED",
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
// Tests
// ---------------------------------------------------------------------------
