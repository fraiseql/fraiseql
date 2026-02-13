//! Operational tools for FraiseQL server
//!
//! Provides production-ready operational infrastructure:
//! - Health check endpoints
//! - Readiness and liveness probes
//! - Metrics collection and export
//! - Graceful shutdown with signal handling

pub mod config;
pub mod health;
pub mod metrics;
pub mod shutdown;

pub use config::{ServerConfig, validate_config};
pub use health::{HealthStatus, health_check};
pub use metrics::{MetricsCollector, metrics_summary};
pub use shutdown::{ShutdownHandler, install_signal_handlers};
