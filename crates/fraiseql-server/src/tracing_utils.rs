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

