//! Distributed tracing support using OpenTelemetry
//!
//! This module provides tracing capabilities for distributed systems, enabling
//! cross-service request tracking and performance analysis via Jaeger/Zipkin.
//!
//! # Examples
//!
//! ```ignore
//! use fraiseql_observers::tracing::{init_tracing, TracingConfig};
//!
//! let config = TracingConfig {
//!     enabled: true,
//!     service_name: "my-observer".to_string(),
//!     jaeger_endpoint: "http://localhost:14268/api/traces".to_string(),
//!     sample_rate: 1.0,
//! };
//!
//! init_tracing(config)?;
//! ```

pub mod config;
pub mod exporter;
pub mod propagation;
pub mod spans;
pub mod instrumentation;
pub mod action_tracing;
pub mod action_integration;

use crate::error::{Error, Result};
use config::TracingConfig;

pub use config::TracingConfig;
pub use exporter::{init_tracing, init_jaeger_exporter, JaegerConfig, JaegerSpan, record_span, flush_spans, is_initialized};
pub use propagation::TraceContext;
pub use spans::{create_event_span, create_action_span, create_phase_span};
pub use instrumentation::{ListenerTracer, ExecutorTracer, ConditionTracer};
pub use action_tracing::{WebhookTracer, EmailTracer, SlackTracer, ActionSpan};
pub use action_integration::{ActionBatchExecutor, ActionChain};

/// Initialize tracing provider with configuration
///
/// This must be called at application startup to enable tracing.
/// If tracing is disabled in config, this is a no-op.
///
/// # Arguments
///
/// * `config` - Tracing configuration
///
/// # Errors
///
/// Returns `Error::Tracing` if initialization fails
///
/// # Example
///
/// ```ignore
/// let config = TracingConfig::from_env()?;
/// fraiseql_observers::tracing::init_tracing(config)?;
/// ```
pub fn init_tracing(config: TracingConfig) -> Result<()> {
    if !config.enabled {
        tracing::debug!("Tracing disabled");
        return Ok(());
    }

    tracing::info!(
        service_name = %config.service_name,
        jaeger_endpoint = %config.jaeger_endpoint,
        sample_rate = config.sample_rate,
        "Initializing tracing"
    );

    exporter::init_jaeger_exporter(&config)?;

    tracing::info!("Tracing initialized successfully");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tracing_init_disabled() {
        let config = TracingConfig {
            enabled: false,
            service_name: "test".to_string(),
            jaeger_endpoint: "http://localhost:14268/api/traces".to_string(),
            sample_rate: 1.0,
        };

        // Should not panic or error
        let result = init_tracing(config);
        assert!(result.is_ok());
    }

    #[test]
    fn test_tracing_config_validation() {
        let config = TracingConfig {
            enabled: true,
            service_name: "test".to_string(),
            jaeger_endpoint: "http://localhost:14268/api/traces".to_string(),
            sample_rate: 0.5,
        };

        assert_eq!(config.service_name, "test");
        assert!(config.sample_rate >= 0.0 && config.sample_rate <= 1.0);
    }
}
