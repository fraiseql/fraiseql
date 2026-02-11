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

/// Initialize all observability components.
///
/// Sets up the `tracing_subscriber` with:
/// - `RUST_LOG` environment variable filter (defaults to
///   `fraiseql_server=info,tower_http=info,axum=info`)
/// - JSON or human-readable format based on `FRAISEQL_LOG_FORMAT` (`json` for JSON, anything else
///   for human-readable)
///
/// This function must be called exactly once, before any tracing macros.
///
/// # Errors
///
/// Returns error if the subscriber cannot be installed.
pub fn init_observability() -> Result<(), Box<dyn std::error::Error>> {
    use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

    let env_filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| "fraiseql_server=info,tower_http=info,axum=info".into());

    let log_format = std::env::var("FRAISEQL_LOG_FORMAT").unwrap_or_default();

    if log_format == "json" {
        tracing_subscriber::registry()
            .with(env_filter)
            .with(tracing_subscriber::fmt::layer().json())
            .init();
    } else {
        tracing_subscriber::registry()
            .with(env_filter)
            .with(tracing_subscriber::fmt::layer())
            .init();
    }

    // Initialize internal tracer and logging helpers
    init_tracer()?;
    init_logging()?;

    Ok(())
}
