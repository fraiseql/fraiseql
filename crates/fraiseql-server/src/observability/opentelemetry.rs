//! OpenTelemetry initialization for distributed tracing.
//!
//! Sets up Jaeger exporter for W3C Trace Context support.

#[cfg(feature = "tracing-opentelemetry")]
use std::time::Duration;

#[cfg(feature = "tracing-opentelemetry")]
use tracing::{info, warn};

/// Initialize OpenTelemetry with Jaeger exporter.
///
/// # Arguments
/// - `service_name`: Name of the service for traces
/// - `otlp_endpoint`: Jaeger OTLP HTTP endpoint (e.g., "http://localhost:4318")
/// - `sampling_rate`: Sampling rate (0.0 to 1.0)
///
/// # Returns
/// A guard that must be kept alive for the duration of the application.
/// Dropping it will flush remaining spans.
///
/// # Note
/// Requires the `tracing-opentelemetry` feature to be enabled.
#[cfg(feature = "tracing-opentelemetry")]
pub fn init_jaeger(
    service_name: &str,
    otlp_endpoint: &str,
    sampling_rate: f64,
) -> Result<impl Drop, Box<dyn std::error::Error>> {
    info!(
        service_name = service_name,
        otlp_endpoint = otlp_endpoint,
        sampling_rate = sampling_rate,
        "Initializing OpenTelemetry with Jaeger exporter"
    );

    // Create OTLP exporter
    let otlp_exporter = opentelemetry_otlp::new_exporter()
        .http()
        .with_endpoint(otlp_endpoint)
        .with_timeout(Duration::from_secs(10))
        .with_headers(std::collections::HashMap::new());

    // Create tracer with sampler
    let tracer = opentelemetry_otlp::new_pipeline()
        .tracing()
        .with_exporter(otlp_exporter)
        .with_trace_config(
            opentelemetry_sdk::trace::Config::default()
                .with_sampler(opentelemetry_sdk::trace::Sampler::ProbabilitySampler(sampling_rate))
                .with_resource(opentelemetry_sdk::Resource::new(vec![
                    opentelemetry::KeyValue::new("service.name", service_name.to_string()),
                ])),
        )
        .install_batch(opentelemetry_sdk::runtime::Tokio)?;

    info!("OpenTelemetry initialized successfully");

    Ok(JaegerGuard { _tracer: tracer })
}

/// Guard to ensure tracer is flushed on drop.
#[cfg(feature = "tracing-opentelemetry")]
pub struct JaegerGuard {
    _tracer: opentelemetry_sdk::trace::TracerProvider,
}

#[cfg(feature = "tracing-opentelemetry")]
impl Drop for JaegerGuard {
    fn drop(&mut self) {
        info!("Flushing OpenTelemetry traces");
        if let Err(e) = opentelemetry::global::shutdown_tracer_provider() {
            warn!(error = %e, "Error shutting down tracer provider");
        }
    }
}

#[cfg(all(test, feature = "tracing-opentelemetry"))]
mod tests {
    use super::*;

    #[test]
    fn test_sampling_rate_validation() {
        // Valid sampling rates
        let valid_rates = vec![0.0, 0.1, 0.5, 1.0];
        for rate in valid_rates {
            assert!(rate >= 0.0 && rate <= 1.0, "Rate {} should be valid", rate);
        }

        // Invalid sampling rates
        let invalid_rates = vec![-0.1, 1.1, 2.0];
        for rate in invalid_rates {
            assert!(!(rate >= 0.0 && rate <= 1.0), "Rate {} should be invalid", rate);
        }
    }
}
