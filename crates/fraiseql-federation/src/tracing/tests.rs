#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable
#![allow(clippy::needless_collect)] // Reason: intermediate collect makes test assertions clearer

use super::*;

#[test]
fn test_federation_trace_context_creation() {
    let ctx = FederationTraceContext::new();
    assert!(!ctx.trace_id.is_empty());
    assert!(!ctx.parent_span_id.is_empty());
    assert_eq!(ctx.trace_flags, "01");
}

#[test]
fn test_federation_trace_context_from_traceparent() {
    let header = "00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01";
    let ctx = FederationTraceContext::from_traceparent(header).unwrap();

    assert_eq!(ctx.trace_id, "4bf92f3577b34da6a3ce929d0e0e4736");
    assert_eq!(ctx.parent_span_id, "00f067aa0ba902b7");
    assert_eq!(ctx.trace_flags, "01");
}

#[test]
fn test_federation_trace_context_to_traceparent() {
    let ctx = FederationTraceContext::new();
    let header = ctx.to_traceparent();

    assert!(header.starts_with("00-"));
    let parts: Vec<&str> = header.split('-').collect();
    assert_eq!(parts.len(), 4);
}

#[test]
fn test_federation_span_creation() {
    let ctx = FederationTraceContext::new();
    let span = FederationSpan::new("federation.query", ctx);

    assert_eq!(span.name, "federation.query");
    assert!(!span.span_id.is_empty());
    assert!(span.duration_ms() >= 0.0);
}

#[test]
fn test_federation_span_attributes() {
    let ctx = FederationTraceContext::new();
    let span = FederationSpan::new("federation.query", ctx)
        .with_attribute("entity_count", "25")
        .with_attribute("max_hops", "3");

    assert_eq!(span.attributes.get("entity_count").unwrap(), "25");
    assert_eq!(span.attributes.get("max_hops").unwrap(), "3");
}

#[test]
fn test_federation_span_create_child() {
    let ctx = FederationTraceContext::new();
    let parent = FederationSpan::new("federation.query", ctx);
    let child = parent.create_child("federation.resolve_db");

    assert_eq!(child.name, "federation.resolve_db");
    assert_eq!(child.parent_span_id, parent.span_id);
    assert_eq!(child.trace_context.trace_id, parent.trace_context.trace_id);
}
