//! Logging integration with trace ID correlation
//!
//! This module provides structured logging with automatic trace ID injection,
//! linking logs to distributed traces for unified debugging.

pub mod correlation;
pub mod structured;

pub use correlation::{TraceIdExtractor, get_current_trace_id, set_trace_id_context};
pub use structured::StructuredLogger;

#[cfg(test)]
mod tests;
