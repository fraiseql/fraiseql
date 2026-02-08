//! Observability and monitoring infrastructure.
//!
//! Provides metrics collection, tracing, and logging capabilities
//! for validation and query execution.

pub mod validation_metrics;

pub use validation_metrics::{ValidationMetricEntry, ValidationMetricsCollector};
