//! Jaeger trace exporter integration
//!
//! Provides integration with Jaeger for distributed tracing via HTTP collector.

use std::sync::Arc;
use std::sync::Mutex;
use std::time::Duration;

use super::config::TracingConfig;
use crate::error::{Error, Result};

/// Jaeger HTTP collector configuration
#[derive(Debug, Clone)]
pub struct JaegerConfig {
    /// Jaeger HTTP endpoint for trace collection
    pub endpoint: String,

    /// Sampling rate (0.0 to 1.0)
    pub sample_rate: f64,

    /// Service name for identification
    pub service_name: String,

    /// Maximum batch size for trace export
    pub max_batch_size: usize,

    /// Export timeout in milliseconds
    pub export_timeout_ms: u64,
}

impl JaegerConfig {
    /// Create Jaeger config from tracing config
    pub fn from_tracing_config(config: &TracingConfig) -> Self {
        Self {
            endpoint: config.jaeger_endpoint.clone(),
            sample_rate: config.sample_rate,
            service_name: config.service_name.clone(),
            max_batch_size: 512,
            export_timeout_ms: 30000,
        }
    }

    /// Validate Jaeger configuration
    ///
    /// # Errors
    ///
    /// Returns [`Error::Tracing`] if the endpoint or service name is empty,
    /// the sample rate is outside `0.0..=1.0`, or `max_batch_size` is 0.
    pub fn validate(&self) -> Result<()> {
        if self.endpoint.is_empty() {
            return Err(Error::Tracing("Jaeger endpoint cannot be empty".to_string()));
        }

        if self.service_name.is_empty() {
            return Err(Error::Tracing(
                "Service name cannot be empty".to_string(),
            ));
        }

        if !(0.0..=1.0).contains(&self.sample_rate) {
            return Err(Error::Tracing(
                format!("Sample rate must be between 0.0 and 1.0, got {}", self.sample_rate),
            ));
        }

        if self.max_batch_size == 0 {
            return Err(Error::Tracing(
                "Max batch size must be greater than 0".to_string(),
            ));
        }

        Ok(())
    }
}

/// Jaeger trace exporter — one instance per Server, not a global singleton.
pub struct JaegerExporter {
    config: JaegerConfig,
    batch_buffer: Arc<Mutex<Vec<JaegerSpan>>>,
}

impl JaegerExporter {
    /// Record a span for export to Jaeger.
    ///
    /// Buffers span data for batch export to the Jaeger collector.
    ///
    /// # Errors
    ///
    /// Returns error if the internal batch buffer lock is poisoned or if
    /// the HTTP export call to Jaeger fails.
    pub fn record_span(&self, span: JaegerSpan) -> Result<()> {
        let mut buffer = self.batch_buffer.lock().expect("batch_buffer mutex poisoned");
        buffer.push(span);
        if buffer.len() >= self.config.max_batch_size {
            let spans_to_export = buffer.drain(..).collect::<Vec<_>>();
            drop(buffer);
            export_spans(&self.config, spans_to_export)?;
        }
        Ok(())
    }

    /// Flush all pending spans to Jaeger.
    ///
    /// # Errors
    ///
    /// Returns error if the HTTP export call to Jaeger fails.
    pub fn flush_spans(&self) -> Result<()> {
        let mut buffer = self.batch_buffer.lock().expect("batch_buffer mutex poisoned");
        if !buffer.is_empty() {
            let spans_to_export = buffer.drain(..).collect::<Vec<_>>();
            drop(buffer);
            export_spans(&self.config, spans_to_export)?;
        }
        tracing::debug!("Flushed pending spans to Jaeger");
        Ok(())
    }

    /// Return the Jaeger configuration for this exporter.
    #[must_use]
    pub fn config(&self) -> &JaegerConfig {
        &self.config
    }
}

/// Simplified span representation for Jaeger export
#[derive(Debug, Clone)]
pub struct JaegerSpan {
    /// Trace ID
    pub trace_id: String,

    /// Span ID
    pub span_id: String,

    /// Parent span ID (if any)
    pub parent_span_id: Option<String>,

    /// Span name/operation
    pub operation_name: String,

    /// Start time in milliseconds
    pub start_time_ms: u64,

    /// Duration in milliseconds
    pub duration_ms: u64,

    /// Span tags (attributes)
    pub tags: Vec<(String, String)>,

    /// Span status (ok, error, unset)
    pub status: String,
}

/// Initialize a Jaeger trace exporter and return it as an owned instance.
///
/// The returned [`JaegerExporter`] should be stored per-`Server` instance
/// rather than in a global. This enables multiple servers to export to
/// different Jaeger endpoints simultaneously.
///
/// # Arguments
///
/// * `config` - Tracing configuration
///
/// # Errors
///
/// Returns error if the config fails validation.
///
/// # Example
///
/// ```no_run
/// # use fraiseql_observers::tracing::TracingConfig;
/// # use fraiseql_observers::tracing::exporter::init_jaeger_exporter;
/// let config = TracingConfig {
///     enabled: true,
///     service_name: "my-service".to_string(),
///     jaeger_endpoint: "http://localhost:14268/api/traces".to_string(),
///     sample_rate: 1.0,
/// };
/// let exporter = init_jaeger_exporter(&config)?;
/// # Ok::<_, fraiseql_observers::error::Error>(())
/// ```
pub fn init_jaeger_exporter(config: &TracingConfig) -> Result<JaegerExporter> {
    config.validate()?;

    let jaeger_config = JaegerConfig::from_tracing_config(config);
    jaeger_config.validate()?;

    tracing::info!(
        service_name = %jaeger_config.service_name,
        endpoint = %jaeger_config.endpoint,
        sample_rate = jaeger_config.sample_rate,
        "Initializing Jaeger exporter"
    );

    let exporter = JaegerExporter {
        config: jaeger_config,
        batch_buffer: Arc::new(Mutex::new(Vec::new())),
    };

    tracing::info!("Jaeger exporter initialized successfully, ready to export traces");

    Ok(exporter)
}

/// Serialize spans to Jaeger's JSON API format.
///
/// Produces a payload suitable for POST to `/api/traces` on the Jaeger HTTP collector.
fn serialize_jaeger_batch(service_name: &str, spans: &[JaegerSpan]) -> serde_json::Value {
    let jaeger_spans: Vec<serde_json::Value> = spans
        .iter()
        .map(|span| {
            let mut tags: Vec<serde_json::Value> = span
                .tags
                .iter()
                .map(|(k, v)| serde_json::json!({"key": k, "type": "string", "value": v}))
                .collect();
            tags.push(serde_json::json!({"key": "status", "type": "string", "value": span.status}));

            let mut obj = serde_json::json!({
                "traceID": span.trace_id,
                "spanID": span.span_id,
                "operationName": span.operation_name,
                // Jaeger expects microseconds
                "startTime": span.start_time_ms * 1000,
                "duration": span.duration_ms * 1000,
                "tags": tags,
                "logs": [],
                "processID": "p1",
                "warnings": null
            });
            if let Some(ref parent) = span.parent_span_id {
                obj["references"] = serde_json::json!([{
                    "refType": "CHILD_OF",
                    "traceID": span.trace_id,
                    "spanID": parent
                }]);
            }
            obj
        })
        .collect();

    serde_json::json!({
        "data": [{
            "traceID": spans.first().map(|s| s.trace_id.as_str()).unwrap_or(""),
            "spans": jaeger_spans,
            "processes": {
                "p1": {
                    "serviceName": service_name,
                    "tags": []
                }
            },
            "warnings": null
        }]
    })
}

/// Export spans to Jaeger HTTP collector.
///
/// Serializes spans to Jaeger's JSON format and POSTs them to the configured
/// endpoint. The HTTP call is spawned as a fire-and-forget tokio task so that
/// export failures never block the calling thread. Errors are logged as warnings.
fn export_spans(config: &JaegerConfig, spans: Vec<JaegerSpan>) -> Result<()> {
    if spans.is_empty() {
        return Ok(());
    }

    let span_count = spans.len();
    tracing::debug!(span_count, endpoint = %config.endpoint, "Exporting spans to Jaeger");

    let payload = serialize_jaeger_batch(&config.service_name, &spans);
    let endpoint = config.endpoint.clone();
    let timeout = Duration::from_millis(config.export_timeout_ms);

    // Fire-and-forget: spawn an async task for the HTTP call so callers are
    // never blocked. Export errors are logged as warnings, not propagated.
    match tokio::runtime::Handle::try_current() {
        Ok(handle) => {
            handle.spawn(async move {
                let client = reqwest::Client::builder()
                    .timeout(timeout)
                    .build()
                    .unwrap_or_else(|e| {
                        tracing::warn!(
                            error = %e,
                            "Failed to build reqwest client for Jaeger exporter; \
                             using default client. Configured timeout will not be applied."
                        );
                        reqwest::Client::default()
                    });

                match client
                    .post(&endpoint)
                    .header("Content-Type", "application/json")
                    .json(&payload)
                    .send()
                    .await
                {
                    Ok(resp) if resp.status().is_success() => {
                        tracing::debug!(span_count, "Spans exported to Jaeger successfully");
                    },
                    Ok(resp) => {
                        tracing::warn!(
                            status = %resp.status(),
                            endpoint = %endpoint,
                            "Jaeger export returned non-success status"
                        );
                    },
                    Err(e) => {
                        tracing::warn!(
                            error = %e,
                            endpoint = %endpoint,
                            "Failed to export spans to Jaeger"
                        );
                    },
                }
            });
        },
        Err(_) => {
            // No tokio runtime available (e.g., called from a sync test context).
            // Log the spans at trace level so they are not silently lost.
            tracing::warn!(
                span_count,
                "No tokio runtime available; spans not sent to Jaeger (logged at trace level)"
            );
            for span in &spans {
                tracing::trace!(
                    trace_id = %span.trace_id,
                    span_id = %span.span_id,
                    operation = %span.operation_name,
                    "Jaeger span (not exported)"
                );
            }
        },
    }

    Ok(())
}


#[cfg(test)]
mod tests {
    use super::*;

    fn make_test_exporter() -> JaegerExporter {
        init_jaeger_exporter(&TracingConfig {
            enabled: true,
            service_name: "test-service".to_string(),
            jaeger_endpoint: "http://localhost:14268/api/traces".to_string(),
            sample_rate: 0.5,
        })
        .expect("test exporter should initialize")
    }

    #[test]
    fn test_jaeger_config_creation() {
        let tracing_config = TracingConfig {
            enabled: true,
            service_name: "my-service".to_string(),
            jaeger_endpoint: "http://localhost:14268/api/traces".to_string(),
            sample_rate: 0.5,
        };

        let jaeger_config = JaegerConfig::from_tracing_config(&tracing_config);

        assert_eq!(jaeger_config.service_name, "my-service");
        assert_eq!(jaeger_config.endpoint, "http://localhost:14268/api/traces");
        assert_eq!(jaeger_config.sample_rate, 0.5);
    }

    #[test]
    fn test_jaeger_config_validation_valid() {
        let config = JaegerConfig {
            endpoint: "http://localhost:14268/api/traces".to_string(),
            sample_rate: 0.5,
            service_name: "test".to_string(),
            max_batch_size: 512,
            export_timeout_ms: 30000,
        };

        config.validate().unwrap_or_else(|e| panic!("expected Ok for valid config: {e}"));
    }

    #[test]
    fn test_jaeger_config_validation_invalid_endpoint() {
        let config = JaegerConfig {
            endpoint: String::new(),
            sample_rate: 0.5,
            service_name: "test".to_string(),
            max_batch_size: 512,
            export_timeout_ms: 30000,
        };

        assert!(
            matches!(config.validate(), Err(crate::error::Error::Tracing(_))),
            "empty endpoint must return Tracing error, got: {:?}",
            config.validate()
        );
    }

    #[test]
    fn test_jaeger_config_validation_invalid_service_name() {
        let config = JaegerConfig {
            endpoint: "http://localhost:14268/api/traces".to_string(),
            sample_rate: 0.5,
            service_name: String::new(),
            max_batch_size: 512,
            export_timeout_ms: 30000,
        };

        assert!(
            matches!(config.validate(), Err(crate::error::Error::Tracing(_))),
            "empty service name must return Tracing error, got: {:?}",
            config.validate()
        );
    }

    #[test]
    fn test_jaeger_config_validation_invalid_sample_rate_high() {
        let config = JaegerConfig {
            endpoint: "http://localhost:14268/api/traces".to_string(),
            sample_rate: 1.5,
            service_name: "test".to_string(),
            max_batch_size: 512,
            export_timeout_ms: 30000,
        };

        assert!(
            matches!(config.validate(), Err(crate::error::Error::Tracing(_))),
            "sample_rate > 1.0 must return Tracing error, got: {:?}",
            config.validate()
        );
    }

    #[test]
    fn test_jaeger_config_validation_invalid_sample_rate_low() {
        let config = JaegerConfig {
            endpoint: "http://localhost:14268/api/traces".to_string(),
            sample_rate: -0.1,
            service_name: "test".to_string(),
            max_batch_size: 512,
            export_timeout_ms: 30000,
        };

        assert!(
            matches!(config.validate(), Err(crate::error::Error::Tracing(_))),
            "sample_rate < 0.0 must return Tracing error, got: {:?}",
            config.validate()
        );
    }

    #[test]
    fn test_jaeger_config_validation_invalid_batch_size() {
        let config = JaegerConfig {
            endpoint: "http://localhost:14268/api/traces".to_string(),
            sample_rate: 0.5,
            service_name: "test".to_string(),
            max_batch_size: 0,
            export_timeout_ms: 30000,
        };

        assert!(
            matches!(config.validate(), Err(crate::error::Error::Tracing(_))),
            "max_batch_size=0 must return Tracing error, got: {:?}",
            config.validate()
        );
    }

    #[test]
    fn test_jaeger_exporter_init_enabled() {
        let config = TracingConfig {
            enabled: true,
            service_name: "test".to_string(),
            jaeger_endpoint: "http://localhost:14268/api/traces".to_string(),
            sample_rate: 1.0,
        };

        let result = init_jaeger_exporter(&config);
        result.unwrap_or_else(|e| panic!("expected Ok for valid enabled config: {e}"));
    }

    #[test]
    fn test_jaeger_exporter_init_disabled() {
        let config = TracingConfig {
            enabled: false,
            service_name: "test".to_string(),
            jaeger_endpoint: "http://localhost:14268/api/traces".to_string(),
            sample_rate: 1.0,
        };

        let result = init_jaeger_exporter(&config);
        // Should succeed because validate() passes even if disabled
        result.unwrap_or_else(|e| panic!("expected Ok even when disabled: {e}"));
    }

    #[test]
    fn test_jaeger_exporter_init_invalid_config() {
        let config = TracingConfig {
            enabled: true,
            service_name: String::new(),
            jaeger_endpoint: "http://localhost:14268/api/traces".to_string(),
            sample_rate: 1.0,
        };

        let result = init_jaeger_exporter(&config);
        assert!(
            matches!(result, Err(crate::error::Error::Tracing(_))),
            "empty service_name must return Tracing error, got: {result:?}"
        );
    }

    #[test]
    fn test_jaeger_span_creation() {
        let span = JaegerSpan {
            trace_id: "a".repeat(32),
            span_id: "b".repeat(16),
            parent_span_id: None,
            operation_name: "process_event".to_string(),
            start_time_ms: 1000,
            duration_ms: 100,
            tags: vec![
                ("event_id".to_string(), "evt-123".to_string()),
                ("status".to_string(), "success".to_string()),
            ],
            status: "ok".to_string(),
        };

        assert_eq!(span.trace_id, "a".repeat(32));
        assert_eq!(span.span_id, "b".repeat(16));
        assert_eq!(span.duration_ms, 100);
        assert_eq!(span.tags.len(), 2);
    }

    #[test]
    fn test_jaeger_span_with_parent() {
        let span = JaegerSpan {
            trace_id: "a".repeat(32),
            span_id: "c".repeat(16),
            parent_span_id: Some("b".repeat(16)),
            operation_name: "execute_action".to_string(),
            start_time_ms: 1100,
            duration_ms: 50,
            tags: vec![("action_type".to_string(), "webhook".to_string())],
            status: "ok".to_string(),
        };

        assert!(span.parent_span_id.is_some());
        assert_eq!(span.parent_span_id.as_ref().unwrap(), "b".repeat(16));
    }

    #[test]
    fn test_get_exporter_config() {
        let exporter = make_test_exporter();
        let config = exporter.config();
        assert_eq!(config.service_name, "test-service");
    }
}
