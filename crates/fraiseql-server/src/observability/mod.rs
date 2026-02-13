//! Observability infrastructure for FraiseQL server
//!
//! Provides OpenTelemetry integration for:
//! - Distributed tracing (spans, trace context)
//! - Structured logging with trace context
//! - Metrics collection (counters, histograms, gauges)
//! - Context propagation across async boundaries

pub mod context;
pub mod logging;
pub mod metrics;
pub mod tracing;

pub use context::{TraceContext, clear_context, get_context, set_context};
pub use logging::init_logging;
pub use metrics::{MetricCounter, MetricHistogram, MetricsRegistry};
pub use tracing::{SpanBuilder, create_span, init_tracer};

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
