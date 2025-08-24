"""Security middleware for Blog Demo Application.

Following enterprise security patterns for proper request validation,
rate limiting, and security headers.
"""

import time
import hashlib
import logging
from typing import Dict, Any
from collections import defaultdict, deque

from fastapi import Request, Response, HTTPException
from starlette.middleware.base import BaseHTTPMiddleware
from starlette.responses import JSONResponse

from ....config import config
from ....core.exceptions import BlogException


logger = logging.getLogger(__name__)


class RateLimiter:
    """Simple in-memory rate limiter."""

    def __init__(self, max_requests: int = 100, window_seconds: int = 60):
        self.max_requests = max_requests
        self.window_seconds = window_seconds
        self.requests: Dict[str, deque] = defaultdict(deque)

    def is_allowed(self, identifier: str) -> bool:
        """Check if request is allowed under rate limit."""
        now = time.time()
        window_start = now - self.window_seconds

        # Get request history for this identifier
        request_times = self.requests[identifier]

        # Remove old requests outside the window
        while request_times and request_times[0] < window_start:
            request_times.popleft()

        # Check if under limit
        if len(request_times) >= self.max_requests:
            return False

        # Add current request
        request_times.append(now)
        return True

    def get_reset_time(self, identifier: str) -> int:
        """Get when rate limit resets for identifier."""
        request_times = self.requests.get(identifier, deque())
        if not request_times:
            return int(time.time())

        oldest_request = request_times[0]
        return int(oldest_request + self.window_seconds)


class SecurityMiddleware(BaseHTTPMiddleware):
    """Security middleware with rate limiting and request validation."""

    def __init__(self, app, rate_limit_requests: int = 100, rate_limit_window: int = 60):
        super().__init__(app)
        self.rate_limiter = RateLimiter(rate_limit_requests, rate_limit_window)
        self.blocked_ips = set()  # In production, load from database/config
        self.allowed_origins = set(config.cors_origins) if config.cors_origins else set()

    async def dispatch(self, request: Request, call_next):
        """Process request with security checks."""

        # Extract client information
        client_ip = self._get_client_ip(request)
        user_agent = request.headers.get("user-agent", "")

        try:
            # IP blocking check
            if client_ip in self.blocked_ips:
                logger.warning(f"Blocked request from IP: {client_ip}")
                return JSONResponse(
                    status_code=403,
                    content={"error": "Access denied"}
                )

            # Rate limiting
            rate_limit_key = f"ip:{client_ip}"
            if not self.rate_limiter.is_allowed(rate_limit_key):
                reset_time = self.rate_limiter.get_reset_time(rate_limit_key)
                logger.warning(f"Rate limit exceeded for IP: {client_ip}")

                return JSONResponse(
                    status_code=429,
                    content={
                        "error": "Rate limit exceeded",
                        "message": "Too many requests. Please try again later.",
                        "reset_time": reset_time
                    },
                    headers={
                        "Retry-After": str(reset_time - int(time.time())),
                        "X-RateLimit-Limit": str(self.rate_limiter.max_requests),
                        "X-RateLimit-Window": str(self.rate_limiter.window_seconds),
                        "X-RateLimit-Reset": str(reset_time),
                    }
                )

            # Content-Type validation for POST requests
            if request.method == "POST":
                content_type = request.headers.get("content-type", "")
                if not content_type.startswith("application/json"):
                    return JSONResponse(
                        status_code=400,
                        content={
                            "error": "Invalid content type",
                            "message": "Content-Type must be application/json"
                        }
                    )

            # Request size validation
            content_length = request.headers.get("content-length")
            if content_length:
                size = int(content_length)
                max_size = 10 * 1024 * 1024  # 10MB limit
                if size > max_size:
                    return JSONResponse(
                        status_code=413,
                        content={
                            "error": "Request too large",
                            "message": f"Request size {size} exceeds limit of {max_size} bytes"
                        }
                    )

            # Add security context to request
            request.state.client_ip = client_ip
            request.state.user_agent = user_agent
            request.state.request_id = self._generate_request_id(request)

            # Process request
            start_time = time.time()
            response = await call_next(request)
            process_time = time.time() - start_time

            # Add security headers
            response.headers["X-Content-Type-Options"] = "nosniff"
            response.headers["X-Frame-Options"] = "DENY"
            response.headers["X-XSS-Protection"] = "1; mode=block"
            response.headers["Referrer-Policy"] = "strict-origin-when-cross-origin"
            response.headers["X-Request-ID"] = request.state.request_id
            response.headers["X-Process-Time"] = f"{process_time:.4f}"

            if not config.debug:
                response.headers["Strict-Transport-Security"] = "max-age=31536000; includeSubDomains"
                response.headers["Content-Security-Policy"] = "default-src 'self'; script-src 'self' 'unsafe-inline'; style-src 'self' 'unsafe-inline'"

            return response

        except Exception as e:
            logger.error(f"Security middleware error: {e}")

            # Don't expose internal errors
            return JSONResponse(
                status_code=500,
                content={
                    "error": "Internal server error",
                    "message": "An unexpected error occurred"
                }
            )

    def _get_client_ip(self, request: Request) -> str:
        """Get client IP address from request."""
        # Check for forwarded IP headers (behind proxy/load balancer)
        forwarded_ips = request.headers.get("X-Forwarded-For")
        if forwarded_ips:
            return forwarded_ips.split(",")[0].strip()

        real_ip = request.headers.get("X-Real-IP")
        if real_ip:
            return real_ip

        # Fall back to direct client IP
        if request.client:
            return request.client.host

        return "unknown"

    def _generate_request_id(self, request: Request) -> str:
        """Generate unique request ID for tracing."""
        timestamp = str(time.time())
        client_ip = self._get_client_ip(request)
        path = str(request.url.path)

        # Create hash from request details
        hash_input = f"{timestamp}-{client_ip}-{path}".encode("utf-8")
        return hashlib.sha256(hash_input).hexdigest()[:16]


class GraphQLSecurityMiddleware(BaseHTTPMiddleware):
    """Additional security specifically for GraphQL endpoints."""

    def __init__(self, app):
        super().__init__(app)
        self.max_query_depth = config.max_query_depth
        self.max_query_complexity = config.max_query_complexity
        self.blocked_operations = {
            "__schema",
            "__type",
            "introspection"
        } if not config.graphql_introspection else set()

    async def dispatch(self, request: Request, call_next):
        """Process GraphQL-specific security checks."""

        # Only apply to GraphQL endpoints
        if not request.url.path.startswith(config.graphql_path):
            return await call_next(request)

        try:
            # Parse GraphQL query for security analysis
            if request.method == "POST":
                body = await request.body()
                if body:
                    import json
                    try:
                        data = json.loads(body.decode("utf-8"))
                        query = data.get("query", "")

                        # Check for blocked operations
                        if any(blocked_op in query for blocked_op in self.blocked_operations):
                            logger.warning(f"Blocked introspection query from {request.state.client_ip}")
                            return JSONResponse(
                                status_code=403,
                                content={
                                    "errors": [{
                                        "message": "Introspection is disabled",
                                        "extensions": {"code": "INTROSPECTION_DISABLED"}
                                    }]
                                }
                            )

                        # Basic query depth check (simplified)
                        depth = query.count("{")
                        if depth > self.max_query_depth:
                            logger.warning(f"Query depth {depth} exceeds limit from {request.state.client_ip}")
                            return JSONResponse(
                                status_code=400,
                                content={
                                    "errors": [{
                                        "message": f"Query depth {depth} exceeds maximum of {self.max_query_depth}",
                                        "extensions": {"code": "QUERY_TOO_DEEP"}
                                    }]
                                }
                            )

                        # Restore body for downstream processing
                        async def receive():
                            return {"type": "http.request", "body": body}

                        request._receive = receive

                    except json.JSONDecodeError:
                        pass  # Let GraphQL handle invalid JSON

            return await call_next(request)

        except Exception as e:
            logger.error(f"GraphQL security middleware error: {e}")
            return JSONResponse(
                status_code=500,
                content={
                    "errors": [{
                        "message": "Internal server error",
                        "extensions": {"code": "INTERNAL_ERROR"}
                    }]
                }
            )
