//! Distributed trace context management
//!
//! Manages trace and span IDs, baggage, and context propagation

use std::collections::HashMap;

use uuid::Uuid;

/// W3C Trace Context
#[derive(Clone, Debug)]
pub struct TraceContext {
    /// Trace ID (32-character hex string)
    pub trace_id:       String,
    /// Span ID (16-character hex string)
    pub span_id:        String,
    /// Parent span ID
    pub parent_span_id: Option<String>,
    /// Baggage items
    pub baggage:        HashMap<String, String>,
    /// Trace flags (sampling decision)
    pub trace_flags:    u8,
}

impl TraceContext {
    /// Create a new root trace context
    pub fn new() -> Self {
        Self {
            trace_id:       Self::generate_trace_id(),
            span_id:        Self::generate_span_id(),
            parent_span_id: None,
            baggage:        HashMap::new(),
            trace_flags:    0x01, // Sampled
        }
    }

    /// Create a child context from this context
    pub fn child(&self) -> Self {
        Self {
            trace_id:       self.trace_id.clone(),
            span_id:        Self::generate_span_id(),
            parent_span_id: Some(self.span_id.clone()),
            baggage:        self.baggage.clone(),
            trace_flags:    self.trace_flags,
        }
    }

    /// Generate a trace ID
    fn generate_trace_id() -> String {
        // Generate 32-character hex string
        let uuid = Uuid::new_v4();
        let hex = uuid.as_bytes();
        let mut id = String::with_capacity(32);

        for byte in hex.iter().take(16) {
            id.push_str(&format!("{:02x}", byte));
        }

        id
    }

    /// Generate a span ID
    fn generate_span_id() -> String {
        // Generate 16-character hex string
        let uuid = Uuid::new_v4();
        let hex = uuid.as_bytes();
        let mut id = String::with_capacity(16);

        for byte in hex.iter().take(8) {
            id.push_str(&format!("{:02x}", byte));
        }

        id
    }

    /// Format as W3C traceparent header
    pub fn traceparent_header(&self) -> String {
        format!("00-{}-{}-{:02x}", self.trace_id, self.span_id, self.trace_flags)
    }

    /// Add baggage item
    pub fn with_baggage(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.baggage.insert(key.into(), value.into());
        self
    }

    /// Parse traceparent header
    pub fn from_traceparent(header: &str) -> Result<Self, String> {
        let parts: Vec<&str> = header.split('-').collect();
        if parts.len() != 4 {
            return Err("Invalid traceparent format".to_string());
        }

        Ok(Self {
            trace_id:       parts[1].to_string(),
            span_id:        parts[2].to_string(),
            parent_span_id: None,
            baggage:        HashMap::new(),
            trace_flags:    u8::from_str_radix(parts[3], 16).map_err(|e| e.to_string())?,
        })
    }
}

impl Default for TraceContext {
    fn default() -> Self {
        Self::new()
    }
}

// Global context storage (thread-local in real implementation)
thread_local! {
    static CONTEXT: std::cell::RefCell<Option<TraceContext>> = const { std::cell::RefCell::new(None) };
}

/// Get current trace context
pub fn get_context() -> Option<TraceContext> {
    CONTEXT.with(|ctx| ctx.borrow().clone())
}

/// Set current trace context
pub fn set_context(context: TraceContext) {
    CONTEXT.with(|ctx| *ctx.borrow_mut() = Some(context));
}

/// Clear current trace context
pub fn clear_context() {
    CONTEXT.with(|ctx| *ctx.borrow_mut() = None);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_trace_context_creation() {
        let ctx = TraceContext::new();
        assert_eq!(ctx.trace_id.len(), 32);
        assert_eq!(ctx.span_id.len(), 16);
    }

    #[test]
    fn test_child_context() {
        let parent = TraceContext::new();
        let child = parent.child();

        assert_eq!(child.trace_id, parent.trace_id);
        assert_ne!(child.span_id, parent.span_id);
        assert_eq!(child.parent_span_id, Some(parent.span_id.clone()));
    }

    #[test]
    fn test_context_get_set() {
        clear_context();
        assert!(get_context().is_none());

        let ctx = TraceContext::new();
        set_context(ctx.clone());
        assert!(get_context().is_some());
    }
}
