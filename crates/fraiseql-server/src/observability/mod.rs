//! Observability infrastructure for FraiseQL server
//!
//! Provides OpenTelemetry integration for:
//! - Distributed tracing (spans, trace context)
//! - Structured logging with trace context
//! - Context propagation across async boundaries

pub mod context;
pub mod logging;
pub mod opentelemetry;

pub use context::{TraceContext, clear_context, get_context, set_context};
pub use logging::init_logging;
