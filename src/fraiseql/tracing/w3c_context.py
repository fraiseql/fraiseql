"""W3C Trace Context support for OpenTelemetry integration (Phase 19, Commit 2).

This module provides utilities for handling W3C Trace Context headers
(traceparent and tracestate) and fallback to custom headers (X-Trace-ID, X-Request-ID).

W3C Trace Context standard: https://www.w3.org/TR/trace-context/
"""

import logging
import uuid
from dataclasses import dataclass

logger = logging.getLogger(__name__)


@dataclass
class TraceContext:
    """Represents a W3C Trace Context."""

    trace_id: str
    """Trace ID (32 hex characters)."""

    span_id: str
    """Parent span ID (16 hex characters)."""

    parent_span_id: str | None = None
    """Parent span ID from traceparent header (16 hex characters)."""

    trace_flags: str = "01"
    """Trace flags (2 hex characters). '01' = sampled."""

    tracestate: str = ""
    """Tracestate header value (optional)."""

    request_id: str | None = None
    """Request ID from X-Request-ID header (for backward compatibility)."""

    def to_traceparent(self) -> str:
        """Convert to W3C traceparent header value.

        Format: version-trace_id-parent_span_id-trace_flags
        Example: 00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01
        """
        return f"00-{self.trace_id}-{self.span_id}-{self.trace_flags}"

    def to_w3c_headers(self) -> dict[str, str]:
        """Convert to W3C trace context headers."""
        headers = {"traceparent": self.to_traceparent()}

        if self.tracestate:
            headers["tracestate"] = self.tracestate

        return headers


def generate_trace_id() -> str:
    """Generate a W3C-compliant trace ID (32 hex characters)."""
    return uuid.uuid4().hex


def generate_span_id() -> str:
    """Generate a W3C-compliant span ID (16 hex characters)."""
    return uuid.uuid4().hex[:16]


def parse_traceparent(traceparent: str) -> dict[str, str] | None:
    """Parse W3C traceparent header.

    Format: version-trace_id-parent_span_id-trace_flags
    Example: 00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01

    Returns:
        Dictionary with trace_id, span_id, and trace_flags, or None if invalid.
    """
    try:
        parts = traceparent.split("-")
        if len(parts) != 4:
            return None

        version, trace_id, span_id, trace_flags = parts

        # Validate version (must be 00 for now)
        if version != "00":
            logger.debug(f"Unsupported traceparent version: {version}")
            return None

        # Validate trace_id (32 hex characters)
        if len(trace_id) != 32 or not all(c in "0123456789abcdef" for c in trace_id):
            logger.debug(f"Invalid trace_id: {trace_id}")
            return None

        # Validate span_id (16 hex characters)
        if len(span_id) != 16 or not all(c in "0123456789abcdef" for c in span_id):
            logger.debug(f"Invalid span_id: {span_id}")
            return None

        # Validate trace_flags (2 hex characters)
        if len(trace_flags) != 2 or not all(c in "0123456789abcdef" for c in trace_flags):
            logger.debug(f"Invalid trace_flags: {trace_flags}")
            return None

        return {
            "trace_id": trace_id,
            "parent_span_id": span_id,
            "trace_flags": trace_flags,
        }
    except Exception as e:
        logger.debug(f"Error parsing traceparent: {e}")
        return None


def extract_trace_context(headers: dict[str, str]) -> TraceContext:
    """Extract trace context from HTTP headers.

    Supports both W3C Trace Context standard and custom headers:
    - W3C: traceparent (required), tracestate (optional)
    - Custom fallback: X-Trace-ID, X-Request-ID

    Args:
        headers: HTTP request headers (case-insensitive dict-like object).

    Returns:
        TraceContext with extracted or generated IDs.
    """
    # Normalize headers to lowercase for case-insensitive lookup
    headers_lower = {k.lower(): v for k, v in headers.items()}

    # Try W3C traceparent first
    traceparent = headers_lower.get("traceparent")
    if traceparent:
        parsed = parse_traceparent(traceparent)
        if parsed:
            return TraceContext(
                trace_id=parsed["trace_id"],
                span_id=generate_span_id(),  # Generate new span ID for this request
                parent_span_id=parsed["parent_span_id"],
                trace_flags=parsed["trace_flags"],
                tracestate=headers_lower.get("tracestate", ""),
                request_id=headers_lower.get("x-request-id"),
            )

    # Fallback to custom headers
    trace_id = headers_lower.get("x-trace-id")
    if trace_id:
        # Pad or truncate to 32 hex characters if needed
        if len(trace_id) < 32:
            trace_id = trace_id.ljust(32, "0")
        elif len(trace_id) > 32:
            trace_id = trace_id[:32]
        # Ensure all characters are valid hex
        try:
            int(trace_id, 16)
        except ValueError:
            trace_id = generate_trace_id()
    else:
        trace_id = generate_trace_id()

    return TraceContext(
        trace_id=trace_id,
        span_id=generate_span_id(),
        request_id=headers_lower.get("x-request-id"),
    )


def inject_trace_context(trace_context: TraceContext) -> dict[str, str]:
    """Inject trace context into HTTP response headers.

    Adds W3C traceparent header to response for downstream propagation.

    Args:
        trace_context: TraceContext to inject.

    Returns:
        Dictionary of headers to add to response.
    """
    return trace_context.to_w3c_headers()
