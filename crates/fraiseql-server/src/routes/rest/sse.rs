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
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
#[allow(clippy::unwrap_used)] // Reason: test code
mod tests {
    use axum::http::HeaderValue;
    use serde_json::json;

    use super::*;

    // -----------------------------------------------------------------------
    // accepts_sse
    // -----------------------------------------------------------------------

    #[test]
    fn accepts_sse_true_for_exact_match() {
        let mut headers = HeaderMap::new();
        headers.insert("accept", HeaderValue::from_static("text/event-stream"));
        assert!(accepts_sse(&headers));
    }

    #[test]
    fn accepts_sse_true_in_list() {
        let mut headers = HeaderMap::new();
        headers.insert("accept", HeaderValue::from_static("application/json, text/event-stream"));
        assert!(accepts_sse(&headers));
    }

    #[test]
    fn accepts_sse_false_for_json() {
        let mut headers = HeaderMap::new();
        headers.insert("accept", HeaderValue::from_static("application/json"));
        assert!(!accepts_sse(&headers));
    }

    #[test]
    fn accepts_sse_false_when_missing() {
        let headers = HeaderMap::new();
        assert!(!accepts_sse(&headers));
    }

    // -----------------------------------------------------------------------
    // is_stream_path / extract_stream_resource
    // -----------------------------------------------------------------------

    #[test]
    fn is_stream_path_true() {
        assert!(is_stream_path("/users/stream"));
    }

    #[test]
    fn is_stream_path_false_collection() {
        assert!(!is_stream_path("/users"));
    }

    #[test]
    fn is_stream_path_false_single() {
        assert!(!is_stream_path("/users/123"));
    }

    #[test]
    fn is_stream_path_false_nested() {
        assert!(!is_stream_path("/users/123/stream/extra"));
    }

    #[test]
    fn extract_stream_resource_users() {
        assert_eq!(extract_stream_resource("/users/stream"), Some("users"));
    }

    #[test]
    fn extract_stream_resource_orders() {
        assert_eq!(extract_stream_resource("/orders/stream"), Some("orders"));
    }

    #[test]
    fn extract_stream_resource_none_for_collection() {
        assert_eq!(extract_stream_resource("/users"), None);
    }

    #[test]
    fn extract_stream_resource_none_for_single() {
        assert_eq!(extract_stream_resource("/users/123"), None);
    }

    // -----------------------------------------------------------------------
    // Last-Event-ID
    // -----------------------------------------------------------------------

    #[test]
    fn extract_last_event_id_present() {
        let mut headers = HeaderMap::new();
        headers.insert("last-event-id", HeaderValue::from_static("evt-42"));
        assert_eq!(extract_last_event_id(&headers), Some("evt-42".to_string()));
    }

    #[test]
    fn extract_last_event_id_missing() {
        let headers = HeaderMap::new();
        assert_eq!(extract_last_event_id(&headers), None);
    }

    // -----------------------------------------------------------------------
    // SSE event formatting
    // -----------------------------------------------------------------------

    #[test]
    fn format_sse_insert_event() {
        let data = json!({"id": 1, "name": "Alice"});
        let output = format_sse_event("insert", "evt-1", &data);
        assert!(output.starts_with("event: insert\n"));
        assert!(output.contains("id: evt-1\n"));
        assert!(output.contains("data: "));
        assert!(output.ends_with("\n\n"));
        // Data line should be valid JSON
        let data_line = output.lines().find(|l| l.starts_with("data: ")).unwrap();
        let json_str = data_line.strip_prefix("data: ").unwrap();
        let parsed: serde_json::Value = serde_json::from_str(json_str).unwrap();
        assert_eq!(parsed["name"], "Alice");
    }

    #[test]
    fn format_sse_update_event() {
        let data = json!({"id": 1, "name": "Alice Updated"});
        let output = format_sse_event("update", "evt-2", &data);
        assert!(output.starts_with("event: update\n"));
    }

    #[test]
    fn format_sse_delete_event() {
        let data = json!({"entity_id": "abc-123"});
        let output = format_sse_event("delete", "evt-3", &data);
        assert!(output.starts_with("event: delete\n"));
        assert!(output.contains("\"entity_id\""));
    }

    #[test]
    fn format_heartbeat_event() {
        let output = format_heartbeat();
        assert!(output.starts_with("event: ping\n"));
        assert!(output.contains("data: \n"));
        assert!(output.ends_with("\n\n"));
    }

    // -----------------------------------------------------------------------
    // event_kind_to_sse_type
    // -----------------------------------------------------------------------

    #[test]
    fn event_kind_insert() {
        assert_eq!(event_kind_to_sse_type("INSERT"), "insert");
    }

    #[test]
    fn event_kind_update() {
        assert_eq!(event_kind_to_sse_type("UPDATE"), "update");
    }

    #[test]
    fn event_kind_delete() {
        assert_eq!(event_kind_to_sse_type("DELETE"), "delete");
    }

    #[test]
    fn event_kind_custom() {
        assert_eq!(event_kind_to_sse_type("CUSTOM"), "custom");
    }

    #[test]
    fn event_kind_unknown() {
        assert_eq!(event_kind_to_sse_type("SOMETHING"), "unknown");
    }

    // -----------------------------------------------------------------------
    // observers_not_available
    // -----------------------------------------------------------------------

    #[test]
    fn observers_not_available_returns_501() {
        let err = observers_not_available();
        assert_eq!(err.status, StatusCode::NOT_IMPLEMENTED);
        assert_eq!(err.code, "NOT_IMPLEMENTED");
    }
}
