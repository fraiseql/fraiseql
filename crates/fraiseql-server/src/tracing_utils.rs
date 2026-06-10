//! Utilities for distributed tracing support.
//!
//! Handles extraction of W3C Trace Context headers from HTTP requests
//! and provides functions for trace context propagation.

use axum::http::HeaderMap;
use fraiseql_core::federation::FederationTraceContext;

/// Extract W3C traceparent header from HTTP headers.
///
/// Parses the standard W3C Trace Context header format:
/// `version-trace_id-parent_span_id-trace_flags`
///
/// Example: `00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01`
///
/// # Arguments
///
/// * `headers` - HTTP headers from the request
///
/// # Returns
///
/// `Some(FederationTraceContext)` if a valid traceparent header is present,
/// `None` otherwise (caller should generate a new trace context).
pub fn extract_trace_context(headers: &HeaderMap) -> Option<FederationTraceContext> {
    headers
        .get("traceparent")
        .and_then(|h| h.to_str().ok())
        .and_then(FederationTraceContext::from_traceparent)
}

/// Extract just the W3C **trace id** (the 32-hex `trace-id` field) from the
/// inbound `traceparent` header — feature-independently, so it is available even
/// in non-federation builds.
///
/// The header format is `version-trace_id-parent_span_id-trace_flags`; this pulls
/// the second field, lower-cases it, and validates it is 32 hex chars and not the
/// all-zero "invalid" trace id. Returns `None` when the header is absent or
/// malformed. Used to stamp the change-log `trace_id` column (#375).
#[must_use]
pub fn extract_trace_id(headers: &HeaderMap) -> Option<String> {
    let value = headers.get("traceparent")?.to_str().ok()?;
    let trace_id = value.split('-').nth(1)?;
    let valid = trace_id.len() == 32
        && trace_id.bytes().all(|b| b.is_ascii_hexdigit())
        && trace_id.bytes().any(|b| b != b'0');
    valid.then(|| trace_id.to_ascii_lowercase())
}

/// Extract the full W3C trace context as a JSON object for the change-log
/// `trace_context` column (#375).
///
/// Feature-independent, so it is available even in non-federation builds (the
/// identical body lives in the `#[cfg(not(feature = "federation"))]` stub in
/// `lib.rs`). Parses the inbound `traceparent` header
/// (`version-trace_id-parent_id-trace_flags`) into
/// `{version, trace_id, parent_id, trace_flags}` (hex fields lower-cased) and adds
/// `tracestate` from the `tracestate` header when present and non-empty. Returns
/// `None` unless the traceparent is well-formed (`trace_id` is 32 hex and not
/// all-zero; `version`/`trace_flags` are 2 hex; `parent_id` is 16 hex) — the same
/// validity as [`extract_trace_id`], so a row's `trace_context` is consistent with
/// its `trace_id`.
#[must_use]
pub fn extract_trace_context_json(headers: &HeaderMap) -> Option<serde_json::Value> {
    let traceparent = headers.get("traceparent")?.to_str().ok()?;
    let mut parts = traceparent.split('-');
    let (version, trace_id, parent_id, trace_flags) =
        (parts.next()?, parts.next()?, parts.next()?, parts.next()?);
    let is_hex = |s: &str, len: usize| s.len() == len && s.bytes().all(|b| b.is_ascii_hexdigit());
    let valid = is_hex(version, 2)
        && is_hex(trace_id, 32)
        && trace_id.bytes().any(|b| b != b'0')
        && is_hex(parent_id, 16)
        && is_hex(trace_flags, 2);
    if !valid {
        return None;
    }
    let mut obj = serde_json::Map::with_capacity(5);
    obj.insert("version".to_owned(), version.to_ascii_lowercase().into());
    obj.insert("trace_id".to_owned(), trace_id.to_ascii_lowercase().into());
    obj.insert("parent_id".to_owned(), parent_id.to_ascii_lowercase().into());
    obj.insert("trace_flags".to_owned(), trace_flags.to_ascii_lowercase().into());
    if let Some(tracestate) = headers
        .get("tracestate")
        .and_then(|h| h.to_str().ok())
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        obj.insert("tracestate".to_owned(), tracestate.into());
    }
    Some(serde_json::Value::Object(obj))
}
