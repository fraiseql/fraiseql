"""Request body size limiting middleware.

Protects against DoS attacks by rejecting requests that exceed
a configurable body size limit.
"""

from dataclasses import dataclass, field
from typing import Any, Callable

from fastapi import Request, Response
from starlette.middleware.base import BaseHTTPMiddleware, RequestResponseEndpoint
from starlette.responses import JSONResponse


class RequestTooLargeError(Exception):
    """Raised when request body exceeds size limit."""

    status_code: int = 413

    def __init__(self, max_size: int, actual_size: int | None = None) -> None:
        self.max_size = max_size
        self.actual_size = actual_size
        self.status_code = 413
        super().__init__(self._format_message())

    def _format_message(self) -> str:
        max_human = _format_bytes(self.max_size)
        if self.actual_size:
            actual_human = _format_bytes(self.actual_size)
            return f"Request body too large: {actual_human} exceeds limit of {max_human}"
        return f"Request body too large: exceeds limit of {max_human}"


def _format_bytes(size: int) -> str:
    """Format bytes as human-readable string."""
    if size >= 1_048_576:
        return f"{size / 1_048_576:.1f} MB"
    if size >= 1024:
        return f"{size / 1024:.1f} KB"
    return f"{size} bytes"


@dataclass
class BodySizeConfig:
    """Configuration for body size limiter.

    Attributes:
        max_body_size: Maximum allowed body size in bytes. Default 1MB.
        exempt_paths: List of paths that bypass the size limit.
        exempt_methods: HTTP methods that bypass the limit.
    """

    max_body_size: int = 1_048_576  # 1MB default
    exempt_paths: list[str] = field(default_factory=list)
    exempt_methods: set[str] = field(default_factory=lambda: {"GET", "HEAD", "OPTIONS"})

    @property
    def human_readable_size(self) -> str:
        """Return max size as human-readable string."""
        return _format_bytes(self.max_body_size)


class SizeLimitedBody:
    """Wrapper that enforces body size limits while streaming.

    SECURITY: This prevents bypass attacks via chunked transfer encoding
    or missing Content-Length headers. The body is measured as it streams.
    """

    def __init__(self, body: Any, max_size: int) -> None:
        self._body = body
        self._max_size = max_size
        self._bytes_read = 0

    async def __aiter__(self):
        async for chunk in self._body:
            self._bytes_read += len(chunk)
            if self._bytes_read > self._max_size:
                raise RequestTooLargeError(
                    max_size=self._max_size,
                    actual_size=self._bytes_read,
                )
            yield chunk


class BodySizeLimiterMiddleware(BaseHTTPMiddleware):
    """Middleware that limits request body size.

    Rejects requests that exceed the configured maximum body size
    with HTTP 413 Payload Too Large.

    SECURITY: Enforces limits in two ways:
    1. Fast path: Check Content-Length header if present
    2. Safe path: Stream body and measure actual bytes (catches chunked/missing header)

    Usage:
        app = FastAPI()
        config = BodySizeConfig(max_body_size=5_000_000)  # 5MB
        app.add_middleware(BodySizeLimiterMiddleware, config=config)
    """

    def __init__(
        self,
        app: Callable,
        config: BodySizeConfig | None = None,
    ) -> None:
        super().__init__(app)
        self.config = config or BodySizeConfig()

    async def dispatch(
        self,
        request: Request,
        call_next: RequestResponseEndpoint,
    ) -> Response:
        """Process request and check body size."""
        # Skip exempt methods
        if request.method in self.config.exempt_methods:
            return await call_next(request)

        # Skip exempt paths
        if request.url.path in self.config.exempt_paths:
            return await call_next(request)

        # Fast path: Check Content-Length header first (efficient)
        content_length = request.headers.get("content-length")
        if content_length:
            try:
                size = int(content_length)
                if size > self.config.max_body_size:
                    return self._create_error_response(size)
            except ValueError:
                pass

        # Safe path: Read and measure actual body (catches chunked transfer, missing headers)
        # This is CRITICAL for security - Content-Length can be omitted or spoofed
        try:
            body = await self._read_body_with_limit(request)
        except RequestTooLargeError as e:
            return self._create_error_response(e.actual_size)

        # Reconstruct request with the body we already read
        # This avoids double-reading the stream
        async def receive():
            return {"type": "http.request", "body": body}

        request._receive = receive

        return await call_next(request)

    async def _read_body_with_limit(self, request: Request) -> bytes:
        """Read request body while enforcing size limit.

        Streams the body and stops early if limit is exceeded,
        preventing memory exhaustion from malicious large requests.
        """
        chunks: list[bytes] = []
        total_size = 0

        async for chunk in request.stream():
            total_size += len(chunk)
            if total_size > self.config.max_body_size:
                raise RequestTooLargeError(
                    max_size=self.config.max_body_size,
                    actual_size=total_size,
                )
            chunks.append(chunk)

        return b"".join(chunks)

    def _create_error_response(self, actual_size: int | None = None) -> JSONResponse:
        """Create 413 error response."""
        error = RequestTooLargeError(
            max_size=self.config.max_body_size,
            actual_size=actual_size,
        )
        return JSONResponse(
            status_code=413,
            content={
                "detail": str(error),
                "max_size": self.config.max_body_size,
                "max_size_human": self.config.human_readable_size,
            },
        )
