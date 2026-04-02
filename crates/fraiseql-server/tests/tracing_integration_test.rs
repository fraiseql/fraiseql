//! `OpenTelemetry` / Tracing integration tests.
//!
//! Validates the tracing infrastructure: W3C `traceparent` extraction,
//! `TracingConfig` deserialization with defaults, and trace context propagation.
//!
//! ## Running Tests
//!
//! ```bash
//! cargo test --test tracing_integration_test --features auth
//! ```

#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable
#![allow(clippy::missing_panics_doc)] // Reason: test helpers
#![allow(clippy::missing_errors_doc)] // Reason: test helpers
#![allow(missing_docs)] // Reason: test code

#[cfg(feature = "federation")]
use axum::http::HeaderMap;
use fraiseql_server::config::tracing::TracingConfig;
#[cfg(feature = "federation")]
use fraiseql_server::tracing_utils::extract_trace_context;

// --- TracingConfig deserialization ---

#[test]
fn tracing_config_defaults() {
    let config = TracingConfig::default();
    assert!(config.enabled);
    assert_eq!(config.level, "info");
    assert_eq!(config.format, "json");
    assert_eq!(config.service_name, "fraiseql");
    assert_eq!(config.otlp_export_timeout_secs, 10);
}

#[test]
fn tracing_config_from_toml_full() {
    let toml_str = r#"
        enabled = true
        level = "debug"
        format = "pretty"
        service_name = "my-fraiseql"
        otlp_export_timeout_secs = 30
    "#;

    let config: TracingConfig = toml::from_str(toml_str).unwrap();
    assert!(config.enabled);
    assert_eq!(config.level, "debug");
    assert_eq!(config.format, "pretty");
    assert_eq!(config.service_name, "my-fraiseql");
    assert_eq!(config.otlp_export_timeout_secs, 30);
}

#[test]
fn tracing_config_from_toml_partial() {
    let toml_str = r#"
        level = "warn"
    "#;

    let config: TracingConfig = toml::from_str(toml_str).unwrap();
    assert!(config.enabled, "should default to enabled");
    assert_eq!(config.level, "warn");
    assert_eq!(config.format, "json", "should default to json");
    assert_eq!(config.service_name, "fraiseql", "should default to fraiseql");
    assert_eq!(config.otlp_export_timeout_secs, 10, "should default to 10");
}

#[test]
fn tracing_config_from_empty_toml() {
    let config: TracingConfig = toml::from_str("").unwrap();
    assert!(config.enabled);
    assert_eq!(config.level, "info");
    assert_eq!(config.format, "json");
}

#[test]
fn tracing_config_disabled() {
    let toml_str = "enabled = false";
    let config: TracingConfig = toml::from_str(toml_str).unwrap();
    assert!(!config.enabled);
}

// --- W3C Trace Context extraction ---
// These tests require the `federation` feature because without it
// `extract_trace_context` returns `Option<()>` (no trace fields).

#[cfg(feature = "federation")]
#[test]
fn extract_valid_traceparent() {
    let mut headers = HeaderMap::new();
    headers.insert(
        "traceparent",
        "00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01".parse().unwrap(),
    );

    let ctx = extract_trace_context(&headers).unwrap();
    assert_eq!(ctx.trace_id, "4bf92f3577b34da6a3ce929d0e0e4736");
    assert_eq!(ctx.parent_span_id, "00f067aa0ba902b7");
    assert_eq!(ctx.trace_flags, "01");
}

#[cfg(feature = "federation")]
#[test]
fn extract_traceparent_unsampled() {
    let mut headers = HeaderMap::new();
    headers.insert(
        "traceparent",
        "00-abcdef1234567890abcdef1234567890-1234567890abcdef-00".parse().unwrap(),
    );

    let ctx = extract_trace_context(&headers).unwrap();
    assert_eq!(ctx.trace_id, "abcdef1234567890abcdef1234567890");
    assert_eq!(ctx.parent_span_id, "1234567890abcdef");
    assert_eq!(ctx.trace_flags, "00");
}

#[cfg(feature = "federation")]
#[test]
fn extract_missing_traceparent_returns_none() {
    let headers = HeaderMap::new();
    assert!(extract_trace_context(&headers).is_none());
}

#[cfg(feature = "federation")]
#[test]
fn extract_invalid_traceparent_returns_none() {
    let mut headers = HeaderMap::new();
    headers.insert("traceparent", "invalid-header".parse().unwrap());
    assert!(extract_trace_context(&headers).is_none());
}

#[cfg(feature = "federation")]
#[test]
fn extract_short_traceparent_still_parses() {
    // The parser only checks for 4 dash-separated parts and version "00".
    // It does NOT validate trace_id/span_id lengths, so short values parse successfully.
    let mut headers = HeaderMap::new();
    headers.insert("traceparent", "00-abc-def-01".parse().unwrap());
    let ctx = extract_trace_context(&headers).unwrap();
    assert_eq!(ctx.trace_id, "abc");
    assert_eq!(ctx.parent_span_id, "def");
    assert_eq!(ctx.trace_flags, "01");
}

#[cfg(feature = "federation")]
#[test]
fn extract_wrong_version_traceparent_returns_none() {
    let mut headers = HeaderMap::new();
    // Version 01 is rejected — only version "00" is accepted.
    headers.insert(
        "traceparent",
        "01-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01".parse().unwrap(),
    );
    assert!(extract_trace_context(&headers).is_none());
}

#[cfg(feature = "federation")]
#[test]
fn extract_empty_traceparent_returns_none() {
    let mut headers = HeaderMap::new();
    headers.insert("traceparent", "".parse().unwrap());
    assert!(extract_trace_context(&headers).is_none());
}

// --- Trace context round-trip ---

#[cfg(feature = "federation")]
#[test]
fn trace_context_to_traceparent_roundtrip() {
    use fraiseql_core::federation::FederationTraceContext;

    let original = FederationTraceContext {
        trace_id:       "4bf92f3577b34da6a3ce929d0e0e4736".to_string(),
        parent_span_id: "00f067aa0ba902b7".to_string(),
        trace_flags:    "01".to_string(),
        query_id:       "test-query-id".to_string(),
    };

    let header_value = original.to_traceparent();

    let mut headers = HeaderMap::new();
    headers.insert("traceparent", header_value.parse().unwrap());

    let extracted = extract_trace_context(&headers).unwrap();
    assert_eq!(extracted.trace_id, original.trace_id);
    assert_eq!(extracted.parent_span_id, original.parent_span_id);
    assert_eq!(extracted.trace_flags, original.trace_flags);
}
