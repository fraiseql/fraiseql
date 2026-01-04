"""Unit tests for W3C Trace Context support (Phase 19, Commit 2).

Tests for parsing, extraction, and injection of W3C Trace Context headers.
"""

import pytest

from fraiseql.tracing.w3c_context import (
    TraceContext,
    extract_trace_context,
    generate_span_id,
    generate_trace_id,
    inject_trace_context,
    parse_traceparent,
)


class TestTraceContextGeneration:
    """Tests for trace ID and span ID generation."""

    def test_generate_trace_id(self) -> None:
        """Test trace ID generation."""
        trace_id = generate_trace_id()
        # Should be 32 hex characters
        assert len(trace_id) == 32
        assert all(c in "0123456789abcdef" for c in trace_id)

    def test_generate_trace_id_unique(self) -> None:
        """Test that generated trace IDs are unique."""
        ids = {generate_trace_id() for _ in range(100)}
        assert len(ids) == 100

    def test_generate_span_id(self) -> None:
        """Test span ID generation."""
        span_id = generate_span_id()
        # Should be 16 hex characters
        assert len(span_id) == 16
        assert all(c in "0123456789abcdef" for c in span_id)

    def test_generate_span_id_unique(self) -> None:
        """Test that generated span IDs are unique."""
        ids = {generate_span_id() for _ in range(100)}
        assert len(ids) == 100


class TestTraceContextDataclass:
    """Tests for TraceContext dataclass."""

    def test_trace_context_creation(self) -> None:
        """Test creating a TraceContext."""
        trace_context = TraceContext(
            trace_id="4bf92f3577b34da6a3ce929d0e0e4736",
            span_id="00f067aa0ba902b7",
        )
        assert trace_context.trace_id == "4bf92f3577b34da6a3ce929d0e0e4736"
        assert trace_context.span_id == "00f067aa0ba902b7"
        assert trace_context.trace_flags == "01"

    def test_trace_context_to_traceparent(self) -> None:
        """Test converting TraceContext to traceparent header."""
        trace_context = TraceContext(
            trace_id="4bf92f3577b34da6a3ce929d0e0e4736",
            span_id="00f067aa0ba902b7",
            trace_flags="01",
        )
        traceparent = trace_context.to_traceparent()
        assert traceparent == "00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01"

    def test_trace_context_to_w3c_headers(self) -> None:
        """Test converting TraceContext to W3C headers."""
        trace_context = TraceContext(
            trace_id="4bf92f3577b34da6a3ce929d0e0e4736",
            span_id="00f067aa0ba902b7",
            tracestate="vendor1=val1,vendor2=val2",
        )
        headers = trace_context.to_w3c_headers()
        assert "traceparent" in headers
        assert "tracestate" in headers
        assert headers["traceparent"] == "00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01"
        assert headers["tracestate"] == "vendor1=val1,vendor2=val2"

    def test_trace_context_without_tracestate(self) -> None:
        """Test TraceContext headers without tracestate."""
        trace_context = TraceContext(
            trace_id="4bf92f3577b34da6a3ce929d0e0e4736",
            span_id="00f067aa0ba902b7",
        )
        headers = trace_context.to_w3c_headers()
        assert "traceparent" in headers
        assert "tracestate" not in headers


class TestParseTraceparent:
    """Tests for parsing W3C traceparent header."""

    def test_parse_valid_traceparent(self) -> None:
        """Test parsing valid traceparent header."""
        traceparent = "00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01"
        result = parse_traceparent(traceparent)
        assert result is not None
        assert result["trace_id"] == "4bf92f3577b34da6a3ce929d0e0e4736"
        assert result["parent_span_id"] == "00f067aa0ba902b7"
        assert result["trace_flags"] == "01"

    def test_parse_traceparent_not_sampled(self) -> None:
        """Test parsing traceparent with not-sampled flag."""
        traceparent = "00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-00"
        result = parse_traceparent(traceparent)
        assert result is not None
        assert result["trace_flags"] == "00"

    def test_parse_invalid_version(self) -> None:
        """Test parsing traceparent with invalid version."""
        traceparent = "01-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01"
        result = parse_traceparent(traceparent)
        # Should reject future versions
        assert result is None

    def test_parse_invalid_trace_id_length(self) -> None:
        """Test parsing traceparent with invalid trace ID length."""
        traceparent = "00-4bf92f3577b34da6a3ce929d0e0e47-00f067aa0ba902b7-01"
        result = parse_traceparent(traceparent)
        assert result is None

    def test_parse_invalid_trace_id_characters(self) -> None:
        """Test parsing traceparent with invalid trace ID characters."""
        traceparent = "00-4bf92f3577b34da6a3ce929d0e0e473g-00f067aa0ba902b7-01"
        result = parse_traceparent(traceparent)
        assert result is None

    def test_parse_invalid_span_id_length(self) -> None:
        """Test parsing traceparent with invalid span ID length."""
        traceparent = "00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902-01"
        result = parse_traceparent(traceparent)
        assert result is None

    def test_parse_invalid_span_id_characters(self) -> None:
        """Test parsing traceparent with invalid span ID characters."""
        traceparent = "00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902bg-01"
        result = parse_traceparent(traceparent)
        assert result is None

    def test_parse_invalid_trace_flags_length(self) -> None:
        """Test parsing traceparent with invalid trace flags length."""
        traceparent = "00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-1"
        result = parse_traceparent(traceparent)
        assert result is None

    def test_parse_invalid_format(self) -> None:
        """Test parsing traceparent with wrong number of parts."""
        traceparent = "00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7"
        result = parse_traceparent(traceparent)
        assert result is None


class TestExtractTraceContext:
    """Tests for extracting trace context from headers."""

    def test_extract_from_w3c_traceparent(self) -> None:
        """Test extracting trace context from W3C traceparent header."""
        headers = {
            "traceparent": "00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01"
        }
        context = extract_trace_context(headers)
        assert context.trace_id == "4bf92f3577b34da6a3ce929d0e0e4736"
        assert context.parent_span_id == "00f067aa0ba902b7"
        assert context.span_id != "00f067aa0ba902b7"  # Should generate new span
        assert len(context.span_id) == 16

    def test_extract_from_w3c_traceparent_with_tracestate(self) -> None:
        """Test extracting trace context with tracestate header."""
        headers = {
            "traceparent": "00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01",
            "tracestate": "vendor1=val1,vendor2=val2",
        }
        context = extract_trace_context(headers)
        assert context.trace_id == "4bf92f3577b34da6a3ce929d0e0e4736"
        assert context.tracestate == "vendor1=val1,vendor2=val2"

    def test_extract_from_custom_trace_id_header(self) -> None:
        """Test extracting trace context from X-Trace-ID header."""
        headers = {"x-trace-id": "abcdef0123456789abcdef0123456789"}
        context = extract_trace_context(headers)
        # Should use the custom trace ID if valid
        assert len(context.trace_id) == 32
        assert context.trace_id == "abcdef0123456789abcdef0123456789"

    def test_extract_from_request_id_header(self) -> None:
        """Test extracting request ID from X-Request-ID header."""
        headers = {
            "traceparent": "00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01",
            "x-request-id": "req-12345",
        }
        context = extract_trace_context(headers)
        assert context.request_id == "req-12345"

    def test_extract_generates_new_ids_when_none_provided(self) -> None:
        """Test that new IDs are generated when no headers provided."""
        headers = {}
        context = extract_trace_context(headers)
        assert len(context.trace_id) == 32
        assert len(context.span_id) == 16
        assert all(c in "0123456789abcdef" for c in context.trace_id)

    def test_extract_case_insensitive_headers(self) -> None:
        """Test that header extraction is case-insensitive."""
        headers = {
            "TraceParent": "00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01",
            "X-TRACE-ID": "custom-trace",
        }
        context = extract_trace_context(headers)
        # Should work with mixed case
        assert context.trace_id == "4bf92f3577b34da6a3ce929d0e0e4736"


class TestInjectTraceContext:
    """Tests for injecting trace context into response headers."""

    def test_inject_trace_context(self) -> None:
        """Test injecting trace context into headers."""
        trace_context = TraceContext(
            trace_id="4bf92f3577b34da6a3ce929d0e0e4736",
            span_id="00f067aa0ba902b7",
        )
        headers = inject_trace_context(trace_context)
        assert "traceparent" in headers
        assert headers["traceparent"] == "00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01"

    def test_inject_with_tracestate(self) -> None:
        """Test injecting trace context with tracestate."""
        trace_context = TraceContext(
            trace_id="4bf92f3577b34da6a3ce929d0e0e4736",
            span_id="00f067aa0ba902b7",
            tracestate="vendor=value",
        )
        headers = inject_trace_context(trace_context)
        assert headers["tracestate"] == "vendor=value"


class TestTraceContextRoundTrip:
    """Tests for round-trip conversion of trace context."""

    def test_extract_inject_roundtrip(self) -> None:
        """Test that extracted and injected headers are compatible."""
        original_headers = {
            "traceparent": "00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01"
        }
        context = extract_trace_context(original_headers)
        injected_headers = inject_trace_context(context)

        # The trace ID should be the same
        assert context.trace_id == "4bf92f3577b34da6a3ce929d0e0e4736"
        # The injected traceparent should have the new span ID
        assert injected_headers["traceparent"].startswith("00-4bf92f3577b34da6a3ce929d0e0e4736-")
