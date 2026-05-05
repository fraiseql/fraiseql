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
    ///
    /// XORs the two 64-bit halves of a single UUID v4 so that the 4 fixed version
    /// bits (byte 6 high nibble) and the 2 fixed variant bits (byte 8 high bits)
    /// are masked by fully-random bytes from the other half, yielding full 64-bit
    /// entropy without requiring additional random generation.
    #[must_use]
    pub fn child_span_id(&self) -> String {
        let uuid_bytes = *uuid::Uuid::new_v4().as_bytes();
        let lo = u64::from_be_bytes(uuid_bytes[0..8].try_into().expect("slice is 8 bytes"));
        let hi = u64::from_be_bytes(uuid_bytes[8..16].try_into().expect("slice is 8 bytes"));
        format!("{:016x}", lo ^ hi)
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
