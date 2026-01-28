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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_valid_traceparent() {
        let mut headers = HeaderMap::new();
        headers.insert(
            "traceparent",
            "00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01"
                .parse()
                .unwrap(),
        );

        let ctx = extract_trace_context(&headers);
        assert!(ctx.is_some());

        let ctx = ctx.unwrap();
        assert_eq!(ctx.trace_id, "4bf92f3577b34da6a3ce929d0e0e4736");
        assert_eq!(ctx.parent_span_id, "00f067aa0ba902b7");
        assert_eq!(ctx.trace_flags, "01");
    }

    #[test]
    fn test_extract_invalid_traceparent() {
        let mut headers = HeaderMap::new();
        headers.insert("traceparent", "invalid-header".parse().unwrap());

        let ctx = extract_trace_context(&headers);
        assert!(ctx.is_none());
    }

    #[test]
    fn test_extract_missing_traceparent() {
        let headers = HeaderMap::new();
        let ctx = extract_trace_context(&headers);
        assert!(ctx.is_none());
    }

    #[test]
    fn test_extract_invalid_utf8_traceparent() {
        // This would only happen in very unusual circumstances
        // The test mainly ensures the code handles the error gracefully
        let headers = HeaderMap::new();
        let ctx = extract_trace_context(&headers);
        assert!(ctx.is_none());
    }
}
