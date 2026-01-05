"""Middleware pipeline for Axum server.

Provides extensible middleware base class and common middleware implementations
for request/response processing, authentication, logging, and rate limiting.
"""

import logging
from abc import ABC, abstractmethod
from typing import Any

logger = logging.getLogger(__name__)


class AxumMiddleware(ABC):
    """Base class for Axum middleware.

    Middleware processes incoming requests and outgoing responses.
    Override process_request() and/or process_response() to implement custom behavior.

    Example:
        ```python
        class CustomMiddleware(AxumMiddleware):
            async def process_request(self, request_data: dict[str, Any]) -> dict[str, Any] | None:
                # Process request
                return request_data

            async def process_response(self, response_data: dict[str, Any]) -> dict[str, Any]:
                # Process response
                return response_data
        ```
    """

    @abstractmethod
    async def process_request(self, request_data: dict[str, Any]) -> dict[str, Any] | None:
        """Process incoming request.

        Args:
            request_data: Request data dictionary containing method, url, headers, body, etc.

        Returns:
            Modified request data, or None to block the request.

        Note:
            Request data structure depends on Axum binding implementation.
            Currently a placeholder for future Axum integration.
        """

    @abstractmethod
    async def process_response(self, response_data: dict[str, Any]) -> dict[str, Any]:
        """Process outgoing response.

        Args:
            response_data: Response data dictionary containing status, headers, body, etc.

        Returns:
            Modified response data.

        Note:
            Response data structure depends on Axum binding implementation.
            Currently a placeholder for future Axum integration.
        """


class RequestLoggingMiddleware(AxumMiddleware):
    """Log all incoming requests.

    Logs request method, URL, and optionally the request body.

    Args:
        log_body: Whether to log request body (default: False)
        log_response: Whether to log response status (default: False)

    Example:
        ```python
        middleware = RequestLoggingMiddleware(log_body=True, log_response=True)
        app.add_middleware(middleware)
        ```
    """

    def __init__(self, log_body: bool = False, log_response: bool = False):
        """Initialize logging middleware.

        Args:
            log_body: Log request body (default: False)
            log_response: Log response status (default: False)
        """
        self.log_body = log_body
        self.log_response = log_response
        logger.debug(
            f"RequestLoggingMiddleware initialized: "
            f"log_body={log_body}, log_response={log_response}",
        )

    async def process_request(self, request_data: dict[str, Any]) -> dict[str, Any] | None:
        """Log incoming request."""
        method = request_data.get("method", "UNKNOWN")
        url = request_data.get("url", "UNKNOWN")
        logger.info(f"{method} {url}")

        if self.log_body:
            body = request_data.get("body", "")
            if body:
                logger.debug(f"Request body: {body}")

        return request_data

    async def process_response(self, response_data: dict[str, Any]) -> dict[str, Any]:
        """Log response status if enabled."""
        if self.log_response:
            status = response_data.get("status", "UNKNOWN")
            logger.info(f"Response status: {status}")

        return response_data


class AuthenticationMiddleware(AxumMiddleware):
    """Require authentication on requests.

    Blocks requests that don't have Authorization header.

    Args:
        header_name: Name of auth header (default: "Authorization")
        optional_paths: Paths that don't require auth (default: [])

    Example:
        ```python
        middleware = AuthenticationMiddleware(
            optional_paths=["/health", "/status"]
        )
        app.add_middleware(middleware)
        ```
    """

    def __init__(self, header_name: str = "Authorization", optional_paths: list[str] | None = None):
        """Initialize authentication middleware.

        Args:
            header_name: Name of authorization header (default: "Authorization")
            optional_paths: Paths that don't require authentication
        """
        self.header_name = header_name
        self.optional_paths = optional_paths or []
        logger.debug(
            f"AuthenticationMiddleware initialized: "
            f"header={header_name}, optional_paths={len(self.optional_paths)}",
        )

    async def process_request(self, request_data: dict[str, Any]) -> dict[str, Any] | None:
        """Check for authentication header."""
        url = request_data.get("url", "")

        # Check if path is optional
        for optional_path in self.optional_paths:
            if url.startswith(optional_path):
                return request_data

        # Check for auth header
        headers = request_data.get("headers", {})
        if self.header_name not in headers:
            logger.warning(f"Request blocked: missing {self.header_name} header")
            return None  # Block request

        return request_data

    async def process_response(self, response_data: dict[str, Any]) -> dict[str, Any]:
        """Pass through response."""
        return response_data


class RateLimitMiddleware(AxumMiddleware):
    """Rate limiting middleware (per IP).

    Tracks request count per IP address and enforces limits.
    Currently a stub - full implementation requires connection tracking.

    Args:
        requests_per_minute: Max requests per minute (default: 100)
        requests_per_hour: Max requests per hour (default: 5000)

    Example:
        ```python
        middleware = RateLimitMiddleware(
            requests_per_minute=1000,
            requests_per_hour=50000
        )
        app.add_middleware(middleware)
        ```
    """

    def __init__(
        self,
        requests_per_minute: int = 100,
        requests_per_hour: int = 5000,
    ):
        """Initialize rate limit middleware.

        Args:
            requests_per_minute: Requests per minute limit
            requests_per_hour: Requests per hour limit
        """
        self.requests_per_minute = requests_per_minute
        self.requests_per_hour = requests_per_hour
        self._ip_counts: dict[str, dict[str, int]] = {}
        logger.debug(
            f"RateLimitMiddleware initialized: "
            f"{requests_per_minute} req/min, {requests_per_hour} req/hr",
        )

    async def process_request(self, request_data: dict[str, Any]) -> dict[str, Any] | None:
        """Check rate limit for request IP."""
        # Get client IP from request
        ip = request_data.get("client_ip", "unknown")

        # Track request count (stub - full implementation needs timestamp tracking)
        if ip not in self._ip_counts:
            self._ip_counts[ip] = {"minute": 0, "hour": 0}

        self._ip_counts[ip]["minute"] += 1
        self._ip_counts[ip]["hour"] += 1

        # For now, just log and allow (full implementation would block)
        if self._ip_counts[ip]["minute"] > self.requests_per_minute:
            logger.warning(f"Rate limit approaching for {ip}: {self._ip_counts[ip]}")

        return request_data

    async def process_response(self, response_data: dict[str, Any]) -> dict[str, Any]:
        """Pass through response."""
        return response_data


class CompressionMiddleware(AxumMiddleware):
    """Response compression middleware.

    Compresses response bodies above a minimum size threshold.
    Currently a stub - full implementation requires HTTP compression library.

    Args:
        algorithm: Compression algorithm ("gzip", "brotli", "deflate") (default: "gzip")
        min_bytes: Minimum response size to compress (default: 256)

    Example:
        ```python
        middleware = CompressionMiddleware(
            algorithm="brotli",
            min_bytes=1024
        )
        app.add_middleware(middleware)
        ```
    """

    def __init__(self, algorithm: str = "gzip", min_bytes: int = 256):
        """Initialize compression middleware.

        Args:
            algorithm: Compression algorithm to use
            min_bytes: Minimum bytes before compressing response
        """
        if algorithm not in ("gzip", "brotli", "deflate"):
            raise ValueError(
                f"Invalid algorithm: {algorithm}. Must be one of: gzip, brotli, deflate",
            )

        self.algorithm = algorithm
        self.min_bytes = min_bytes
        logger.debug(
            f"CompressionMiddleware initialized: algorithm={algorithm}, min_bytes={min_bytes}",
        )

    async def process_request(self, request_data: dict[str, Any]) -> dict[str, Any] | None:
        """Pass through request."""
        return request_data

    async def process_response(self, response_data: dict[str, Any]) -> dict[str, Any]:
        """Apply compression to response if needed."""
        # Stub: Full implementation would compress based on min_bytes threshold
        # For now, just log and return
        body_size = len(str(response_data.get("body", "")))
        if body_size > self.min_bytes:
            logger.debug(f"Response of {body_size} bytes would be compressed with {self.algorithm}")

        return response_data


class MiddlewarePipeline:
    """Manages ordered execution of middleware.

    Executes middleware in order for requests and reverse order for responses.

    Example:
        ```python
        pipeline = MiddlewarePipeline([
            RequestLoggingMiddleware(),
            AuthenticationMiddleware(),
            RateLimitMiddleware(),
        ])

        # Process request through all middleware
        request = {"method": "GET", "url": "/graphql"}
        result = await pipeline.process_request(request)
        ```
    """

    def __init__(self, middleware: list[AxumMiddleware] | None = None):
        """Initialize middleware pipeline.

        Args:
            middleware: List of middleware instances in execution order
        """
        self.middleware = middleware or []
        logger.debug(f"Initialized MiddlewarePipeline with {len(self.middleware)} middleware")

    def add(self, middleware: AxumMiddleware) -> None:
        """Add middleware to pipeline.

        Args:
            middleware: Middleware instance to add
        """
        self.middleware.append(middleware)
        logger.debug(f"Added middleware: {middleware.__class__.__name__}")

    async def process_request(self, request_data: dict[str, Any]) -> dict[str, Any] | None:
        """Process request through all middleware.

        Executes middleware in order. If any middleware returns None,
        request is blocked and processing stops.

        Args:
            request_data: Request data dictionary

        Returns:
            Modified request data, or None if request was blocked
        """
        current_request = request_data

        for mw in self.middleware:
            if current_request is None:
                logger.debug(f"Request blocked by {mw.__class__.__name__}")
                break

            current_request = await mw.process_request(current_request)

        return current_request

    async def process_response(self, response_data: dict[str, Any]) -> dict[str, Any]:
        """Process response through all middleware (reverse order).

        Executes middleware in reverse order to ensure responses
        are unwrapped in reverse of how requests were wrapped.

        Args:
            response_data: Response data dictionary

        Returns:
            Modified response data
        """
        current_response = response_data

        # Process in reverse order
        for mw in reversed(self.middleware):
            current_response = await mw.process_response(current_response)

        return current_response
