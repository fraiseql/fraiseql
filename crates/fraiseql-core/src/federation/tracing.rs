//! Distributed tracing support for federation operations.
//!
//! Provides W3C Trace Context support and span creation for federation queries.

use std::time::Instant;

use uuid::Uuid;

/// Federation trace context following W3C Trace Context format.
#[derive(Debug, Clone)]
pub struct FederationTraceContext {
    /// Unique trace identifier (128-bit)
    pub trace_id: String,

    /// Parent span identifier (64-bit)
    pub parent_span_id: String,

    /// Trace flags (sampling decision)
    pub trace_flags: String,

    /// Query ID for correlation
    pub query_id: String,
}

impl FederationTraceContext {
    /// Create new trace context with random IDs.
    pub fn new() -> Self {
        Self {
            trace_id:       format!("{:032x}", Uuid::new_v4().as_u128()),
            parent_span_id: format!("{:016x}", Uuid::new_v4().as_u64_pair().0),
            trace_flags:    "01".to_string(), // sampled
            query_id:       Uuid::new_v4().to_string(),
        }
    }

    /// Create from W3C traceparent header.
    ///
    /// Format: version-trace_id-parent_span_id-trace_flags
    /// Example: 00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01
    pub fn from_traceparent(header: &str) -> Option<Self> {
        let parts: Vec<&str> = header.split('-').collect();
        if parts.len() != 4 {
            return None;
        }

        // Validate version (should be 00)
        if parts[0] != "00" {
            return None;
        }

        Some(Self {
            trace_id:       parts[1].to_string(),
            parent_span_id: parts[2].to_string(),
            trace_flags:    parts[3].to_string(),
            query_id:       Uuid::new_v4().to_string(),
        })
    }

    /// Generate W3C traceparent header value.
    pub fn to_traceparent(&self) -> String {
        format!("00-{}-{}-{}", self.trace_id, self.parent_span_id, self.trace_flags)
    }

    /// Create child span ID for next hop.
    pub fn next_span_id(&self) -> String {
        format!("{:016x}", Uuid::new_v4().as_u64_pair().0)
    }
}

impl Default for FederationTraceContext {
    fn default() -> Self {
        Self::new()
    }
}

/// Federation span for tracking operation timing and metadata.
#[derive(Debug, Clone)]
pub struct FederationSpan {
    /// Span name (e.g., "federation.query.execute")
    pub name: String,

    /// Span ID
    pub span_id: String,

    /// Parent span ID
    pub parent_span_id: String,

    /// Trace context
    pub trace_context: FederationTraceContext,

    /// Span start time
    pub start_time: Instant,

    /// Span attributes
    pub attributes: std::collections::HashMap<String, String>,
}

impl FederationSpan {
    /// Create new federation span.
    pub fn new(name: impl Into<String>, trace_context: FederationTraceContext) -> Self {
        let parent_span_id = trace_context.parent_span_id.clone();
        Self {
            name: name.into(),
            span_id: trace_context.next_span_id(),
            parent_span_id,
            trace_context,
            start_time: Instant::now(),
            attributes: std::collections::HashMap::new(),
        }
    }

    /// Add attribute to span.
    pub fn with_attribute(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.attributes.insert(key.into(), value.into());
        self
    }

    /// Get span duration in milliseconds.
    pub fn duration_ms(&self) -> f64 {
        self.start_time.elapsed().as_secs_f64() * 1000.0
    }

    /// Create child span.
    pub fn create_child(&self, name: impl Into<String>) -> Self {
        let mut child_context = self.trace_context.clone();
        child_context.parent_span_id.clone_from(&self.span_id);

        FederationSpan::new(name, child_context)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_federation_trace_context_creation() {
        let ctx = FederationTraceContext::new();
        assert!(!ctx.trace_id.is_empty());
        assert!(!ctx.parent_span_id.is_empty());
        assert_eq!(ctx.trace_flags, "01");
    }

    #[test]
    fn test_federation_trace_context_from_traceparent() {
        let header = "00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01";
        let ctx = FederationTraceContext::from_traceparent(header).unwrap();

        assert_eq!(ctx.trace_id, "4bf92f3577b34da6a3ce929d0e0e4736");
        assert_eq!(ctx.parent_span_id, "00f067aa0ba902b7");
        assert_eq!(ctx.trace_flags, "01");
    }

    #[test]
    fn test_federation_trace_context_to_traceparent() {
        let ctx = FederationTraceContext::new();
        let header = ctx.to_traceparent();

        assert!(header.starts_with("00-"));
        let parts: Vec<&str> = header.split('-').collect();
        assert_eq!(parts.len(), 4);
    }

    #[test]
    fn test_federation_span_creation() {
        let ctx = FederationTraceContext::new();
        let span = FederationSpan::new("federation.query", ctx);

        assert_eq!(span.name, "federation.query");
        assert!(!span.span_id.is_empty());
        assert!(span.duration_ms() >= 0.0);
    }

    #[test]
    fn test_federation_span_attributes() {
        let ctx = FederationTraceContext::new();
        let span = FederationSpan::new("federation.query", ctx)
            .with_attribute("entity_count", "25")
            .with_attribute("max_hops", "3");

        assert_eq!(span.attributes.get("entity_count").unwrap(), "25");
        assert_eq!(span.attributes.get("max_hops").unwrap(), "3");
    }

    #[test]
    fn test_federation_span_create_child() {
        let ctx = FederationTraceContext::new();
        let parent = FederationSpan::new("federation.query", ctx);
        let child = parent.create_child("federation.resolve_db");

        assert_eq!(child.name, "federation.resolve_db");
        assert_eq!(child.parent_span_id, parent.span_id);
        assert_eq!(child.trace_context.trace_id, parent.trace_context.trace_id);
    }
}
