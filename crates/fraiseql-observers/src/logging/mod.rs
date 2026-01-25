//! Logging integration with trace ID correlation
//!
//! This module provides structured logging with automatic trace ID injection,
//! linking logs to distributed traces for unified debugging.

pub mod correlation;
pub mod structured;

pub use correlation::{TraceIdExtractor, get_current_trace_id, set_trace_id_context};
pub use structured::StructuredLogger;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_trace_id_correlation() {
        set_trace_id_context("test-trace-id-123");
        let trace_id = get_current_trace_id();
        assert_eq!(trace_id, Some("test-trace-id-123".to_string()));
    }

    #[test]
    fn test_clear_trace_id() {
        set_trace_id_context("some-trace-id");
        set_trace_id_context("");
        let trace_id = get_current_trace_id();
        assert_eq!(trace_id, None);
    }
}
