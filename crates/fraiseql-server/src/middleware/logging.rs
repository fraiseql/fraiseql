//! Request logging middleware for structured JSON logging.
//!
//! Automatically logs request and response details in structured JSON format,
//! capturing request context, performance metrics, and error information.

use axum::{
    extract::Request,
    http::HeaderMap,
    middleware::Next,
    response::Response,
};
use std::time::Instant;
use tracing::debug;

use crate::logging::{LogLevel, LogMetrics, RequestContext, RequestId, StructuredLogEntry};

/// Logging middleware for structured request/response logging.
///
/// Captures request details and automatically logs metrics upon response.
pub async fn logging_middleware(
    headers: HeaderMap,
    request: Request,
    next: Next,
) -> Response {
    // Generate request ID
    let _request_id = RequestId::new();

    // Extract client IP from headers or connection info
    let client_ip = headers
        .get("x-forwarded-for")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("unknown")
        .to_string();

    // Create request context
    let context = RequestContext::new()
        .with_client_ip(client_ip.clone());

    // Log incoming request
    let path = request.uri().path().to_string();
    let method = request.method().to_string();

    let log_entry = StructuredLogEntry::new(
        LogLevel::Debug,
        format!("Incoming {method} request to {path}"),
    )
    .with_request_context(context.clone());

    debug!("{}", log_entry.to_json_string());

    // Measure request processing time
    let start = Instant::now();
    let response = next.run(request).await;
    let duration_ms = start.elapsed().as_secs_f64() * 1000.0;

    // Log response with metrics
    let status = response.status().as_u16();
    let level = if status >= 500 {
        LogLevel::Error
    } else if status >= 400 {
        LogLevel::Warn
    } else {
        LogLevel::Info
    };

    let metrics = LogMetrics::new().with_duration_ms(duration_ms);

    let response_log = StructuredLogEntry::new(
        level,
        format!("{method} {path} response with status {status}"),
    )
    .with_request_context(context)
    .with_metrics(metrics);

    debug!("{}", response_log.to_json_string());

    response
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_request_id_generation() {
        let id1 = RequestId::new();
        let id2 = RequestId::new();
        assert_ne!(id1.to_string(), id2.to_string());
    }

    #[test]
    fn test_request_context_with_ip() {
        let context = RequestContext::new()
            .with_client_ip("192.168.1.100".to_string());

        assert_eq!(context.client_ip, Some("192.168.1.100".to_string()));
    }

    #[test]
    fn test_log_entry_json_output() {
        let context = RequestContext::new()
            .with_client_ip("10.0.0.1".to_string());

        let entry = StructuredLogEntry::new(LogLevel::Info, "test message".to_string())
            .with_request_context(context);

        let json = entry.to_json_string();
        assert!(json.contains("\"message\":\"test message\""));
        assert!(json.contains("\"level\":\"INFO\""));
    }

    #[test]
    fn test_metrics_duration_capture() {
        let metrics = LogMetrics::new().with_duration_ms(123.45);
        assert_eq!(metrics.duration_ms, Some(123.45));
    }
}
