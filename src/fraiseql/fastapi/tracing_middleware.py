"""Request tracing middleware for W3C Trace Context support (Phase 19, Commit 2).

Middleware for propagating trace context through HTTP requests using W3C standards.
"""

import time
from typing import Any

from fastapi import FastAPI, Request
from starlette.middleware.base import BaseHTTPMiddleware

from fraiseql.fastapi.config import FraiseQLConfig
from fraiseql.fastapi.dependencies import get_fraiseql_config
from fraiseql.tracing.w3c_context import extract_trace_context, inject_trace_context


class RequestTracingMiddleware(BaseHTTPMiddleware):
    """Middleware for request tracing with W3C Trace Context support.

    Extracts trace context from incoming request headers, propagates it through
    the request lifecycle, and injects it into response headers for downstream
    services.

    Supports both W3C Trace Context standard headers and custom headers
    (X-Trace-ID, X-Request-ID) for backward compatibility.
    """

    def __init__(self, app: Any, config: FraiseQLConfig | None = None) -> None:
        """Initialize tracing middleware.

        Args:
            app: FastAPI application instance.
            config: FraiseQL configuration (optional, loaded from dependencies).
        """
        super().__init__(app)
        self.config = config

    async def dispatch(self, request: Request, call_next: Any) -> Any:
        """Process request with tracing context.

        Args:
            request: HTTP request object.
            call_next: Next middleware/handler in chain.

        Returns:
            HTTP response with trace context injected.
        """
        # Get config if not provided
        config = self.config
        if config is None:
            try:
                config = get_fraiseql_config()
            except RuntimeError:
                config = None

        # Check if tracing is enabled
        if config and not config.tracing_enabled:
            return await call_next(request)

        # Extract trace context from request headers
        trace_context = extract_trace_context(dict(request.headers))

        # Store trace context in request state for downstream access
        request.state.trace_context = trace_context
        request.state.trace_id = trace_context.trace_id
        request.state.span_id = trace_context.span_id
        request.state.request_id = trace_context.request_id or trace_context.trace_id

        # Check sampling decision
        config_sample_rate = config.trace_sample_rate if config else 1.0
        should_sample = trace_context.trace_flags == "01" and (
            config_sample_rate >= 1.0 or time.time() % 1.0 < config_sample_rate
        )
        request.state.should_sample = should_sample

        # Process request
        response = await call_next(request)

        # Inject trace context into response headers
        if config and config.tracing_enabled:
            trace_headers = inject_trace_context(trace_context)
            for header_name, header_value in trace_headers.items():
                response.headers[header_name] = header_value

        return response


def setup_tracing_middleware(app: FastAPI, config: FraiseQLConfig | None = None) -> None:
    """Set up request tracing middleware for the FastAPI application.

    Args:
        app: FastAPI application instance.
        config: FraiseQL configuration (optional).
    """
    if config is None:
        try:
            config = get_fraiseql_config()
        except RuntimeError:
            config = None

    # Only add middleware if tracing is enabled
    if config and config.tracing_enabled:
        app.add_middleware(RequestTracingMiddleware, config=config)
