"""Framework-agnostic HTTP server abstraction layer.

This module defines the core protocols that all HTTP server implementations
(Axum, Starlette, FastAPI) must follow. The abstraction is minimal and derived
from the production Axum implementation.

Architecture:
- Request parsing: Convert framework-specific requests to GraphQLRequest
- Response formatting: Convert GraphQLResponse to framework-specific responses
- Middleware: Framework-agnostic middleware protocols
- Health checking: Standard health check implementation
- Subscriptions: WebSocket subscription protocol (graphql-ws)

Key Principle:
The abstraction is EXTRACTED FROM AXUM CODE, not theoretical.
This ensures all servers can implement it without issues.
"""

from dataclasses import dataclass, field
from typing import Any, AsyncIterator, Dict, Optional, Protocol, runtime_checkable

# ============================================================================
# Core GraphQL Request/Response Types
# ============================================================================


@dataclass
class GraphQLRequest:
    """Standard GraphQL request format.

    Extracted from fraiseql_rs/src/http/axum_server.rs:GraphQLRequest
    This is the canonical format used by all HTTP servers.

    Attributes:
        query: The GraphQL query string
        operation_name: Optional operation name if multiple operations defined
        variables: Optional query variables as dict
        extensions: Optional extensions (used for APQ, tracing, etc.)
    """

    query: str
    operation_name: Optional[str] = None
    variables: Optional[Dict[str, Any]] = None
    extensions: Optional[Dict[str, Any]] = None

    def validate(self) -> None:
        """Validate that query is present.

        Raises:
            ValueError: If query is missing or empty
        """
        if not self.query or not self.query.strip():
            raise ValueError("GraphQL query is required")


@dataclass
class GraphQLError:
    """Standard GraphQL error format.

    Extracted from fraiseql_rs/src/http/axum_server.rs:GraphQLError

    Attributes:
        message: The error message
        extensions: Optional error extensions (e.g., code, context)
    """

    message: str
    extensions: Optional[Dict[str, Any]] = None

    def to_dict(self) -> Dict[str, Any]:
        """Convert to dictionary for JSON serialization."""
        result = {"message": self.message}
        if self.extensions:
            result["extensions"] = self.extensions
        return result


@dataclass
class GraphQLResponse:
    """Standard GraphQL response format.

    Extracted from fraiseql_rs/src/http/axum_server.rs:GraphQLResponse
    This is what all handlers return before framework-specific formatting.

    Attributes:
        data: The query result data (None if errors)
        errors: List of errors (if any)
        status_code: HTTP status code (200 for success, 400+ for errors)
    """

    data: Optional[Dict[str, Any]] = None
    errors: Optional[list[GraphQLError]] = None
    status_code: int = 200

    def to_dict(self) -> Dict[str, Any]:
        """Convert to dictionary for JSON serialization."""
        result = {}
        if self.data is not None:
            result["data"] = self.data
        if self.errors:
            result["errors"] = [e.to_dict() for e in self.errors]
        return result

    @classmethod
    def success(cls, data: Dict[str, Any]) -> "GraphQLResponse":
        """Create a successful response."""
        return cls(data=data, status_code=200)

    @classmethod
    def error(cls, message: str, code: Optional[int] = None) -> "GraphQLResponse":
        """Create an error response."""
        return cls(
            errors=[GraphQLError(message=message)],
            status_code=code or 400,
        )


# ============================================================================
# Framework-Agnostic Context
# ============================================================================


@dataclass
class HttpContext:
    """Framework-agnostic HTTP request context.

    This is the wrapper used to pass framework-specific request data
    while maintaining a clean abstraction boundary.

    Attributes:
        request_body: Parsed JSON body (usually Dict[str, Any])
        headers: HTTP headers as dict
        user: Optional user context (from authentication)
        method: HTTP method (GET, POST, etc.)
        path: Request path
        raw_request: Raw framework request object (for framework-specific code)
        extra: Framework-specific data dict
    """

    request_body: Dict[str, Any]
    headers: Dict[str, str]
    user: Optional[Any] = None
    method: str = "POST"
    path: str = "/graphql"
    raw_request: Optional[Any] = None
    extra: Dict[str, Any] = field(default_factory=dict)

    def get_extra(self, key: str, default: Any = None) -> Any:
        """Get framework-specific data."""
        return self.extra.get(key, default)

    def set_extra(self, key: str, value: Any) -> None:
        """Set framework-specific data."""
        self.extra[key] = value


# ============================================================================
# Request Parsing Protocol
# ============================================================================


@runtime_checkable
class RequestParser(Protocol):
    """Parse framework-specific requests to standard GraphQLRequest.

    Each HTTP framework implements this to handle its specific request format.

    Examples:
    - Axum: Extracts from axum::Json<GraphQLRequest>
    - Starlette: Parses from request.json()
    - FastAPI: Extracts from pydantic model

    Note: This protocol is derived from how Axum handles requests
    in fraiseql_rs/src/http/axum_server.rs
    """

    async def parse_graphql_request(self, context: HttpContext) -> GraphQLRequest:
        """Parse HTTP context to GraphQL request.

        Args:
            context: Framework-agnostic HTTP context

        Returns:
            GraphQLRequest with query, variables, etc.

        Raises:
            ValueError: If request body is invalid
            json.JSONDecodeError: If JSON parsing fails
        """
        ...


# ============================================================================
# Response Formatting Protocol
# ============================================================================


@runtime_checkable
class ResponseFormatter(Protocol):
    """Format GraphQLResponse to framework-specific response.

    Each HTTP framework implements this to return responses in its format.

    Examples:
    - Axum: Returns axum::Json<GraphQLResponse>
    - Starlette: Returns JSONResponse
    - FastAPI: Returns JSONResponse

    Note: This protocol is derived from how Axum formats responses
    in fraiseql_rs/src/http/axum_server.rs
    """

    async def format_response(self, response: GraphQLResponse) -> Any:
        """Format GraphQL response to framework response.

        Args:
            response: Standard GraphQL response

        Returns:
            Framework-specific response object

        Example (Starlette):
            return JSONResponse(
                response.to_dict(),
                status_code=response.status_code,
            )
        """
        ...


# ============================================================================
# Middleware Protocol
# ============================================================================


@runtime_checkable
class HttpMiddleware(Protocol):
    """Framework-agnostic middleware for request/response processing.

    Middleware can modify requests before execution and responses after.
    Order matters: middleware is applied in registration order.

    Examples:
    - Authentication: Extract and validate auth headers
    - Logging: Log request/response details
    - Metrics: Collect performance metrics
    - Caching: Cache query results (if enabled)

    Note: This is an abstraction over how Axum handles middleware
    in fraiseql_rs/src/http/middleware.rs
    """

    async def process_request(self, context: HttpContext) -> HttpContext:
        """Process request before GraphQL execution.

        Can modify context, validate, add user data, etc.

        Args:
            context: HTTP context to process

        Returns:
            Modified HTTP context

        Raises:
            PermissionError: If request should be rejected (e.g., auth failure)
            ValueError: If request is invalid
        """
        ...

    async def process_response(
        self, response: GraphQLResponse, context: HttpContext
    ) -> GraphQLResponse:
        """Process response after GraphQL execution.

        Can modify response, add headers, cache results, etc.

        Args:
            response: GraphQL response to process
            context: HTTP context (for logging, auth info, etc.)

        Returns:
            Modified GraphQL response
        """
        ...


# ============================================================================
# Health Check Protocol
# ============================================================================


@dataclass
class HealthStatus:
    """Health check response.

    Matches format used by Axum in fraiseql_rs/src/http/optimization.rs

    Attributes:
        status: "healthy" or "unhealthy"
        version: Server version
        details: Optional health details
    """

    status: str
    version: str
    details: Optional[Dict[str, Any]] = None

    def to_dict(self) -> Dict[str, Any]:
        """Convert to dictionary for JSON response."""
        result = {
            "status": self.status,
            "version": self.version,
        }
        if self.details:
            result.update(self.details)
        return result


@runtime_checkable
class HealthChecker(Protocol):
    """Framework-agnostic health check.

    Each HTTP framework calls this to check server health.

    Examples:
    - Axum: Calls in health_check handler
    - Starlette: Calls in /health route
    - FastAPI: Calls in @app.get("/health")

    Note: Derived from fraiseql_rs/src/http/optimization.rs
    """

    async def check_health(self) -> HealthStatus:
        """Check server health.

        Returns:
            HealthStatus with status, version, and optional details

        Raises:
            Exception: If health check fails (will return unhealthy status)
        """
        ...


# ============================================================================
# Subscription Protocol (WebSocket)
# ============================================================================


@runtime_checkable
class SubscriptionHandler(Protocol):
    """Framework-agnostic WebSocket subscription handling.

    Handles graphql-ws protocol for GraphQL subscriptions.

    Each framework implements this for its WebSocket API:
    - Axum: Uses axum::extract::ws::WebSocket
    - Starlette: Uses Starlette's WebSocket
    - FastAPI: Uses FastAPI's WebSocket

    Note: Derived from fraiseql_rs/src/http/websocket.rs
    """

    async def handle_subscription(self, context: HttpContext) -> AsyncIterator[GraphQLResponse]:
        """Handle WebSocket subscription.

        Args:
            context: HTTP context with subscription request

        Yields:
            GraphQL responses as events arrive

        Raises:
            ValueError: If subscription request is invalid
            ConnectionError: If WebSocket connection is lost
        """
        ...


# ============================================================================
# Framework Integration Entry Point
# ============================================================================


class HttpServer:
    """Base class for HTTP server implementations.

    Subclasses implement this for Axum, Starlette, FastAPI, etc.
    The shared business logic calls these methods to handle requests
    in a framework-agnostic way.

    Example:
        class AxumServer(HttpServer):
            async def handle_graphql(self, context):
                # Axum-specific handling
                response = await execute_graphql_request(context, ...)
                return response

        class StarletteServer(HttpServer):
            async def handle_graphql(self, context):
                # Starlette-specific handling
                response = await execute_graphql_request(context, ...)
                return response
    """

    async def handle_graphql(self, context: HttpContext) -> GraphQLResponse:
        """Handle GraphQL request.

        Args:
            context: HTTP context with request details

        Returns:
            GraphQL response

        Raises:
            ValueError: If request is invalid
            Exception: If execution fails
        """
        raise NotImplementedError

    async def handle_health(self) -> HealthStatus:
        """Handle health check request.

        Returns:
            Health status
        """
        raise NotImplementedError

    async def handle_subscription(self, context: HttpContext) -> AsyncIterator[GraphQLResponse]:
        """Handle WebSocket subscription.

        Args:
            context: HTTP context with subscription request

        Yields:
            GraphQL responses

        Raises:
            ValueError: If subscription request is invalid
        """
        raise NotImplementedError


# ============================================================================
# Export all public types
# ============================================================================

__all__ = [
    "GraphQLError",
    "GraphQLRequest",
    "GraphQLResponse",
    "HealthChecker",
    "HealthStatus",
    "HttpContext",
    "HttpMiddleware",
    "HttpServer",
    "RequestParser",
    "ResponseFormatter",
    "SubscriptionHandler",
]
