//! W3C Trace Context propagation for distributed tracing
//!
//! Implements trace context propagation following the W3C Trace Context standard
//! (https://www.w3.org/TR/trace-context/) to enable cross-service tracing.

use std::collections::HashMap;

/// Trace context information for distributed tracing
///
/// Implements W3C Trace Context format for propagation across service boundaries.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TraceContext {
    /// Trace ID (16 bytes, 32 hex chars)
    pub trace_id: String,

    /// Span ID (8 bytes, 16 hex chars)
    pub span_id: String,

    /// Trace flags (sampling decision)
    /// - Bit 0: Sampled flag
    pub trace_flags: u8,

    /// Trace state (vendor-specific)
    pub trace_state: Option<String>,
}

impl TraceContext {
    /// Create a new trace context
    pub fn new(trace_id: String, span_id: String, trace_flags: u8) -> Self {
        Self {
            trace_id,
            span_id,
            trace_flags,
            trace_state: None,
        }
    }

    /// Check if tracing is sampled
    pub fn is_sampled(&self) -> bool {
        (self.trace_flags & 0x01) != 0
    }

    /// Convert to W3C Trace Context header format
    ///
    /// Format: `00-{trace_id}-{span_id}-{flags}`
    /// Example: `00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01`
    pub fn to_traceparent_header(&self) -> String {
        format!(
            "00-{}-{}-{:02x}",
            self.trace_id, self.span_id, self.trace_flags
        )
    }

    /// Convert to HTTP headers map
    pub fn to_headers(&self) -> HashMap<String, String> {
        let mut headers = HashMap::new();
        headers.insert("traceparent".to_string(), self.to_traceparent_header());

        if let Some(ref state) = self.trace_state {
            headers.insert("tracestate".to_string(), state.clone());
        }

        headers
    }

    /// Parse W3C Trace Context from traceparent header
    ///
    /// # Arguments
    ///
    /// * `header` - traceparent header value
    ///
    /// # Returns
    ///
    /// Parsed trace context or None if invalid format
    ///
    /// # Example
    ///
    /// ```
    /// use fraiseql_observers::tracing::TraceContext;
    ///
    /// let header = "00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01";
    /// let ctx = TraceContext::from_traceparent_header(header);
    ///
    /// assert!(ctx.is_some());
    /// ```
    pub fn from_traceparent_header(header: &str) -> Option<Self> {
        let parts: Vec<&str> = header.split('-').collect();

        // Version-00 traceparent MUST contain exactly 4 dash-separated fields.
        // Per W3C Trace Context spec §3.2.1: if version is "00" and the header
        // has more than 4 components it MUST be considered invalid.
        // The `tracestate` value is carried by a separate `tracestate` header,
        // not embedded in `traceparent`.
        if parts.len() != 4 {
            return None;
        }

        let version = parts[0];
        if version != "00" {
            return None;
        }

        let trace_id = parts[1].to_string();
        let span_id = parts[2].to_string();

        let trace_flags = u8::from_str_radix(parts[3], 16).ok()?;

        // Validate trace_id: 32 lowercase hex chars, all-zeros invalid (W3C §3.3.2)
        if trace_id.len() != 32
            || !trace_id.chars().all(|c| c.is_ascii_hexdigit())
            || trace_id.chars().all(|c| c == '0')
        {
            return None;
        }

        // Validate span_id: 16 lowercase hex chars, all-zeros invalid (W3C §3.3.3)
        if span_id.len() != 16
            || !span_id.chars().all(|c| c.is_ascii_hexdigit())
            || span_id.chars().all(|c| c == '0')
        {
            return None;
        }

        Some(Self {
            trace_id,
            span_id,
            trace_flags,
            trace_state: None, // tracestate is a separate header; see from_headers()
        })
    }

    /// Extract trace context from HTTP headers
    pub fn from_headers(headers: &HashMap<String, String>) -> Option<Self> {
        let traceparent = headers.get("traceparent")?;
        let mut ctx = Self::from_traceparent_header(traceparent)?;

        if let Some(state) = headers.get("tracestate") {
            ctx.trace_state = Some(state.clone());
        }

        Some(ctx)
    }

    /// Generate a new child span ID for this context.
    ///
    /// Creates a cryptographically random W3C-compliant span ID (8 bytes, 16 hex chars)
    /// while maintaining the same trace ID.
    #[must_use]
    pub fn child_span_id(&self) -> String {
        let uuid_bytes = *uuid::Uuid::new_v4().as_bytes();
        format!(
            "{:016x}",
            u64::from_be_bytes([
                uuid_bytes[0],
                uuid_bytes[1],
                uuid_bytes[2],
                uuid_bytes[3],
                uuid_bytes[4],
                uuid_bytes[5],
                uuid_bytes[6],
                uuid_bytes[7],
            ])
        )
    }
}

impl Default for TraceContext {
    /// Returns a new root trace context with cryptographically random, W3C-valid IDs.
    ///
    /// The W3C Trace Context spec explicitly forbids all-zero `trace-id` and `span-id`
    /// values, so they are generated using random UUIDs.
    fn default() -> Self {
        let trace_bytes = *uuid::Uuid::new_v4().as_bytes();
        let span_bytes = *uuid::Uuid::new_v4().as_bytes();

        let trace_id = trace_bytes.iter().fold(String::with_capacity(32), |mut s, b| {
            let _ = std::fmt::Write::write_fmt(&mut s, format_args!("{b:02x}"));
            s
        });

        let span_id = u64::from_be_bytes([
            span_bytes[0],
            span_bytes[1],
            span_bytes[2],
            span_bytes[3],
            span_bytes[4],
            span_bytes[5],
            span_bytes[6],
            span_bytes[7],
        ]);

        Self {
            trace_id,
            span_id: format!("{span_id:016x}"),
            trace_flags: 0x00,
            trace_state: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_trace_context_new() {
        let ctx = TraceContext::new(
            "4bf92f3577b34da6a3ce929d0e0e4736".to_string(),
            "00f067aa0ba902b7".to_string(),
            0x01,
        );

        assert_eq!(ctx.trace_id, "4bf92f3577b34da6a3ce929d0e0e4736");
        assert_eq!(ctx.span_id, "00f067aa0ba902b7");
        assert_eq!(ctx.trace_flags, 0x01);
        assert!(ctx.is_sampled());
    }

    #[test]
    fn test_is_sampled() {
        let sampled = TraceContext::new(
            "a".repeat(32),
            "b".repeat(16),
            0x01,
        );
        assert!(sampled.is_sampled());

        let not_sampled = TraceContext::new(
            "a".repeat(32),
            "b".repeat(16),
            0x00,
        );
        assert!(!not_sampled.is_sampled());
    }

    #[test]
    fn test_to_traceparent_header() {
        let ctx = TraceContext::new(
            "4bf92f3577b34da6a3ce929d0e0e4736".to_string(),
            "00f067aa0ba902b7".to_string(),
            0x01,
        );

        let header = ctx.to_traceparent_header();
        assert_eq!(
            header,
            "00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01"
        );
    }

    #[test]
    fn test_to_headers() {
        let ctx = TraceContext::new(
            "a".repeat(32),
            "b".repeat(16),
            0x01,
        );

        let headers = ctx.to_headers();
        assert!(headers.contains_key("traceparent"));
        assert_eq!(
            headers["traceparent"],
            format!("00-{}-{}-01", "a".repeat(32), "b".repeat(16))
        );
    }

    #[test]
    fn test_from_traceparent_header_valid() {
        let header = "00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01";
        let ctx = TraceContext::from_traceparent_header(header);

        assert!(ctx.is_some());
        let ctx = ctx.unwrap();
        assert_eq!(ctx.trace_id, "4bf92f3577b34da6a3ce929d0e0e4736");
        assert_eq!(ctx.span_id, "00f067aa0ba902b7");
        assert_eq!(ctx.trace_flags, 0x01);
        assert!(ctx.is_sampled());
    }

    #[test]
    fn test_from_traceparent_header_invalid_version() {
        let header = "01-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01";
        let ctx = TraceContext::from_traceparent_header(header);
        assert!(ctx.is_none());
    }

    #[test]
    fn test_from_traceparent_header_invalid_trace_id() {
        let header = "00-invalid-00f067aa0ba902b7-01";
        let ctx = TraceContext::from_traceparent_header(header);
        assert!(ctx.is_none());
    }

    #[test]
    fn test_from_traceparent_header_invalid_span_id() {
        let header = "00-4bf92f3577b34da6a3ce929d0e0e4736-invalid-01";
        let ctx = TraceContext::from_traceparent_header(header);
        assert!(ctx.is_none());
    }

    #[test]
    fn test_from_traceparent_header_extra_fields_rejected_for_version_00() {
        // W3C spec §3.2.1: version-00 with extra dash-separated components is invalid.
        // tracestate is carried by a separate header, not embedded in traceparent.
        let header = "00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01-vendor=value";
        let ctx = TraceContext::from_traceparent_header(header);
        assert!(ctx.is_none(), "version-00 headers with 5+ parts must be rejected");
    }

    #[test]
    fn test_from_headers_tracestate_from_separate_header() {
        // tracestate must come from the tracestate header, not from traceparent.
        let mut headers = std::collections::HashMap::new();
        headers.insert(
            "traceparent".to_string(),
            "00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01".to_string(),
        );
        headers.insert("tracestate".to_string(), "vendor=value".to_string());

        let ctx = TraceContext::from_headers(&headers).unwrap();
        assert_eq!(ctx.trace_state, Some("vendor=value".to_string()));
    }

    #[test]
    fn test_from_traceparent_header_rejects_all_zero_trace_id() {
        let header = format!("00-{}-00f067aa0ba902b7-01", "0".repeat(32));
        assert!(TraceContext::from_traceparent_header(&header).is_none());
    }

    #[test]
    fn test_from_traceparent_header_rejects_all_zero_span_id() {
        let header = format!("00-4bf92f3577b34da6a3ce929d0e0e4736-{}-01", "0".repeat(16));
        assert!(TraceContext::from_traceparent_header(&header).is_none());
    }

    #[test]
    fn test_from_headers() {
        let mut headers = HashMap::new();
        headers.insert(
            "traceparent".to_string(),
            "00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01".to_string(),
        );
        headers.insert(
            "tracestate".to_string(),
            "vendor=value".to_string(),
        );

        let ctx = TraceContext::from_headers(&headers);

        assert!(ctx.is_some());
        let ctx = ctx.unwrap();
        assert_eq!(ctx.trace_id, "4bf92f3577b34da6a3ce929d0e0e4736");
        assert_eq!(ctx.trace_state, Some("vendor=value".to_string()));
    }

    #[test]
    fn test_child_span_id() {
        let ctx = TraceContext::new(
            "a".repeat(32),
            "0000000000000001".to_string(),
            0x01,
        );

        let child_id = ctx.child_span_id();
        assert_ne!(child_id, ctx.span_id);
        assert_eq!(child_id.len(), 16);
    }

    #[test]
    fn test_default_produces_valid_ids() {
        let ctx = TraceContext::default();
        // W3C spec: trace_id must be 32 hex chars, non-zero
        assert_eq!(ctx.trace_id.len(), 32);
        assert!(ctx.trace_id.chars().all(|c| c.is_ascii_hexdigit()));
        assert!(!ctx.trace_id.chars().all(|c| c == '0'), "all-zero trace_id is invalid");
        // span_id must be 16 hex chars, non-zero
        assert_eq!(ctx.span_id.len(), 16);
        assert!(ctx.span_id.chars().all(|c| c.is_ascii_hexdigit()));
        // Two defaults must differ (astronomically unlikely to collide)
        let ctx2 = TraceContext::default();
        assert_ne!(ctx.trace_id, ctx2.trace_id);
    }
}
