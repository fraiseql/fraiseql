//! Operational tools for FraiseQL server
//!
//! Provides production-ready operational infrastructure:
//! - Health check endpoints
//! - Readiness and liveness probes
//! - Metrics collection and export
//! - Graceful shutdown with signal handling

pub mod health;
pub mod metrics;
pub mod config;
pub mod shutdown;

pub use health::{HealthStatus, health_check};
pub use metrics::{MetricsCollector, metrics_summary};
pub use config::{validate_config, ServerConfig};
pub use shutdown::{ShutdownHandler, install_signal_handlers};
