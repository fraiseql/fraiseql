//! Observability infrastructure for FraiseQL server
//!
//! Provides OpenTelemetry integration for:
//! - Distributed tracing (spans, trace context)
//! - Structured logging with trace context
//! - Metrics collection (counters, histograms, gauges)
//! - Context propagation across async boundaries

pub mod tracing;
pub mod metrics;
pub mod logging;
pub mod context;

pub use tracing::{init_tracer, create_span, SpanBuilder};
pub use metrics::{MetricsRegistry, MetricCounter, MetricHistogram};
pub use logging::init_logging;
pub use context::{TraceContext, get_context, set_context, clear_context};

/// Initialize all observability components
///
/// # Errors
///
/// Returns error if initialization fails
pub fn init_observability() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize tracer
    init_tracer()?;

    // Initialize logging
    init_logging()?;

    Ok(())
}
