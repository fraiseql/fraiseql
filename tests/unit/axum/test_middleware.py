"""Unit tests for middleware pipeline."""

import pytest

from fraiseql.axum.middleware import (
    AuthenticationMiddleware,
    AxumMiddleware,
    CompressionMiddleware,
    MiddlewarePipeline,
    RateLimitMiddleware,
    RequestLoggingMiddleware,
)


class TestAxumMiddlewareBase:
    """Test abstract middleware base class."""

    def test_cannot_instantiate_directly(self) -> None:
        """Test that AxumMiddleware cannot be instantiated directly."""
        with pytest.raises(TypeError):
            AxumMiddleware()  # type: ignore[abstract]

    def test_requires_process_request_implementation(self) -> None:
        """Test that subclasses must implement process_request."""

        class IncompleteMiddleware(AxumMiddleware):
            async def process_response(self, response_data: dict) -> dict:
                return response_data

        with pytest.raises(TypeError):
            IncompleteMiddleware()  # type: ignore[abstract]

    def test_requires_process_response_implementation(self) -> None:
        """Test that subclasses must implement process_response."""

        class IncompleteMiddleware(AxumMiddleware):
            async def process_request(self, request_data: dict) -> dict | None:
                return request_data

        with pytest.raises(TypeError):
            IncompleteMiddleware()  # type: ignore[abstract]


class TestRequestLoggingMiddleware:
    """Test request logging middleware."""

    def test_initialization_default(self) -> None:
        """Test default initialization."""
        middleware = RequestLoggingMiddleware()

        assert middleware.log_body is False
        assert middleware.log_response is False

    def test_initialization_with_options(self) -> None:
        """Test initialization with options."""
        middleware = RequestLoggingMiddleware(log_body=True, log_response=True)

        assert middleware.log_body is True
        assert middleware.log_response is True

    @pytest.mark.asyncio
    async def test_process_request_basic(self) -> None:
        """Test basic request logging."""
        middleware = RequestLoggingMiddleware()
        request = {"method": "GET", "url": "/graphql"}

        result = await middleware.process_request(request)

        assert result == request
        assert result is not None

    @pytest.mark.asyncio
    async def test_process_request_with_body_logging(self) -> None:
        """Test request logging with body."""
        middleware = RequestLoggingMiddleware(log_body=True)
        request = {"method": "POST", "url": "/graphql", "body": '{"query": "..."}'}

        result = await middleware.process_request(request)

        assert result == request

    @pytest.mark.asyncio
    async def test_process_response_with_logging(self) -> None:
        """Test response logging."""
        middleware = RequestLoggingMiddleware(log_response=True)
        response = {"status": 200, "body": "OK"}

        result = await middleware.process_response(response)

        assert result == response

    @pytest.mark.asyncio
    async def test_process_response_without_logging(self) -> None:
        """Test response pass-through without logging."""
        middleware = RequestLoggingMiddleware(log_response=False)
        response = {"status": 200, "body": "OK"}

        result = await middleware.process_response(response)

        assert result == response

    @pytest.mark.asyncio
    async def test_handles_missing_fields(self) -> None:
        """Test handling of missing request fields."""
        middleware = RequestLoggingMiddleware()
        request = {}  # Empty request

        result = await middleware.process_request(request)

        assert result == request


class TestAuthenticationMiddleware:
    """Test authentication middleware."""

    def test_initialization_default(self) -> None:
        """Test default initialization."""
        middleware = AuthenticationMiddleware()

        assert middleware.header_name == "Authorization"
        assert middleware.optional_paths == []

    def test_initialization_with_options(self) -> None:
        """Test initialization with custom header and optional paths."""
        middleware = AuthenticationMiddleware(
            header_name="X-API-Key", optional_paths=["/health", "/status"]
        )

        assert middleware.header_name == "X-API-Key"
        assert middleware.optional_paths == ["/health", "/status"]

    @pytest.mark.asyncio
    async def test_blocks_request_without_auth(self) -> None:
        """Test that request without auth header is blocked."""
        middleware = AuthenticationMiddleware()
        request = {"method": "GET", "url": "/graphql", "headers": {}}

        result = await middleware.process_request(request)

        assert result is None

    @pytest.mark.asyncio
    async def test_allows_request_with_auth(self) -> None:
        """Test that request with auth header is allowed."""
        middleware = AuthenticationMiddleware()
        request = {"method": "GET", "url": "/graphql", "headers": {"Authorization": "Bearer token"}}

        result = await middleware.process_request(request)

        assert result == request

    @pytest.mark.asyncio
    async def test_allows_optional_paths_without_auth(self) -> None:
        """Test that optional paths don't require auth."""
        middleware = AuthenticationMiddleware(optional_paths=["/health", "/status"])
        request = {"method": "GET", "url": "/health", "headers": {}}

        result = await middleware.process_request(request)

        assert result == request

    @pytest.mark.asyncio
    async def test_requires_auth_for_non_optional_paths(self) -> None:
        """Test that non-optional paths require auth."""
        middleware = AuthenticationMiddleware(optional_paths=["/health"])
        request = {"method": "GET", "url": "/graphql", "headers": {}}

        result = await middleware.process_request(request)

        assert result is None

    @pytest.mark.asyncio
    async def test_custom_header_name(self) -> None:
        """Test custom authentication header."""
        middleware = AuthenticationMiddleware(header_name="X-API-Key")
        request = {"method": "GET", "url": "/graphql", "headers": {"X-API-Key": "secret"}}

        result = await middleware.process_request(request)

        assert result == request

    @pytest.mark.asyncio
    async def test_process_response_passthrough(self) -> None:
        """Test that response is passed through unchanged."""
        middleware = AuthenticationMiddleware()
        response = {"status": 200, "body": "OK"}

        result = await middleware.process_response(response)

        assert result == response


class TestRateLimitMiddleware:
    """Test rate limiting middleware."""

    def test_initialization_default(self) -> None:
        """Test default initialization."""
        middleware = RateLimitMiddleware()

        assert middleware.requests_per_minute == 100
        assert middleware.requests_per_hour == 5000

    def test_initialization_with_options(self) -> None:
        """Test initialization with custom limits."""
        middleware = RateLimitMiddleware(requests_per_minute=1000, requests_per_hour=50000)

        assert middleware.requests_per_minute == 1000
        assert middleware.requests_per_hour == 50000

    @pytest.mark.asyncio
    async def test_process_request_allows_request(self) -> None:
        """Test that requests below limit are allowed."""
        middleware = RateLimitMiddleware(requests_per_minute=100)
        request = {"method": "GET", "url": "/graphql", "client_ip": "192.168.1.1"}

        result = await middleware.process_request(request)

        assert result == request

    @pytest.mark.asyncio
    async def test_tracks_requests_per_ip(self) -> None:
        """Test that requests are tracked per IP."""
        middleware = RateLimitMiddleware()
        request1 = {"method": "GET", "url": "/graphql", "client_ip": "192.168.1.1"}
        request2 = {"method": "GET", "url": "/graphql", "client_ip": "192.168.1.2"}

        await middleware.process_request(request1)
        await middleware.process_request(request1)
        await middleware.process_request(request2)

        assert middleware._ip_counts["192.168.1.1"]["minute"] == 2
        assert middleware._ip_counts["192.168.1.2"]["minute"] == 1

    @pytest.mark.asyncio
    async def test_process_response_passthrough(self) -> None:
        """Test that response is passed through unchanged."""
        middleware = RateLimitMiddleware()
        response = {"status": 200, "body": "OK"}

        result = await middleware.process_response(response)

        assert result == response


class TestCompressionMiddleware:
    """Test compression middleware."""

    def test_initialization_default(self) -> None:
        """Test default initialization."""
        middleware = CompressionMiddleware()

        assert middleware.algorithm == "gzip"
        assert middleware.min_bytes == 256

    def test_initialization_with_options(self) -> None:
        """Test initialization with custom options."""
        middleware = CompressionMiddleware(algorithm="brotli", min_bytes=1024)

        assert middleware.algorithm == "brotli"
        assert middleware.min_bytes == 1024

    def test_initialization_with_invalid_algorithm(self) -> None:
        """Test that invalid algorithms are rejected."""
        with pytest.raises(ValueError):
            CompressionMiddleware(algorithm="invalid")

    def test_valid_algorithms(self) -> None:
        """Test all valid compression algorithms."""
        for algo in ("gzip", "brotli", "deflate"):
            middleware = CompressionMiddleware(algorithm=algo)
            assert middleware.algorithm == algo

    @pytest.mark.asyncio
    async def test_process_request_passthrough(self) -> None:
        """Test that request is passed through unchanged."""
        middleware = CompressionMiddleware()
        request = {"method": "POST", "url": "/graphql"}

        result = await middleware.process_request(request)

        assert result == request

    @pytest.mark.asyncio
    async def test_process_response_small_response(self) -> None:
        """Test that small responses are not compressed."""
        middleware = CompressionMiddleware(min_bytes=256)
        response = {"status": 200, "body": "small"}

        result = await middleware.process_response(response)

        assert result == response

    @pytest.mark.asyncio
    async def test_process_response_large_response(self) -> None:
        """Test handling of large responses."""
        middleware = CompressionMiddleware(min_bytes=100)
        response = {"status": 200, "body": "x" * 500}

        result = await middleware.process_response(response)

        assert result == response


class TestMiddlewarePipeline:
    """Test middleware pipeline execution."""

    def test_initialization_empty(self) -> None:
        """Test initialization with no middleware."""
        pipeline = MiddlewarePipeline()

        assert pipeline.middleware == []

    def test_initialization_with_middleware(self) -> None:
        """Test initialization with middleware list."""
        mw1 = RequestLoggingMiddleware()
        mw2 = AuthenticationMiddleware()
        pipeline = MiddlewarePipeline([mw1, mw2])

        assert len(pipeline.middleware) == 2
        assert pipeline.middleware[0] == mw1
        assert pipeline.middleware[1] == mw2

    def test_add_middleware(self) -> None:
        """Test adding middleware to pipeline."""
        pipeline = MiddlewarePipeline()
        middleware = RequestLoggingMiddleware()

        pipeline.add(middleware)

        assert len(pipeline.middleware) == 1
        assert pipeline.middleware[0] == middleware

    @pytest.mark.asyncio
    async def test_process_request_empty_pipeline(self) -> None:
        """Test processing request with no middleware."""
        pipeline = MiddlewarePipeline()
        request = {"method": "GET", "url": "/graphql"}

        result = await pipeline.process_request(request)

        assert result == request

    @pytest.mark.asyncio
    async def test_process_request_single_middleware(self) -> None:
        """Test processing request with single middleware."""
        pipeline = MiddlewarePipeline([RequestLoggingMiddleware()])
        request = {"method": "GET", "url": "/graphql"}

        result = await pipeline.process_request(request)

        assert result == request

    @pytest.mark.asyncio
    async def test_process_request_multiple_middleware(self) -> None:
        """Test processing request through multiple middleware."""
        pipeline = MiddlewarePipeline(
            [
                RequestLoggingMiddleware(),
                AuthenticationMiddleware(),
            ]
        )
        request = {
            "method": "GET",
            "url": "/graphql",
            "headers": {"Authorization": "Bearer token"},
        }

        result = await pipeline.process_request(request)

        assert result == request

    @pytest.mark.asyncio
    async def test_process_request_blocked(self) -> None:
        """Test that middleware can block request."""
        pipeline = MiddlewarePipeline([AuthenticationMiddleware()])
        request = {"method": "GET", "url": "/graphql", "headers": {}}

        result = await pipeline.process_request(request)

        assert result is None

    @pytest.mark.asyncio
    async def test_process_request_stops_at_block(self) -> None:
        """Test that request processing stops when blocked."""
        logging_called = False

        class TrackingMiddleware(RequestLoggingMiddleware):
            async def process_request(self, request_data: dict) -> dict | None:
                nonlocal logging_called
                logging_called = True
                return await super().process_request(request_data)

        pipeline = MiddlewarePipeline(
            [
                AuthenticationMiddleware(),
                TrackingMiddleware(),
            ]
        )
        request = {"method": "GET", "url": "/graphql", "headers": {}}

        result = await pipeline.process_request(request)

        assert result is None
        assert not logging_called  # Should not reach logging middleware

    @pytest.mark.asyncio
    async def test_process_response_empty_pipeline(self) -> None:
        """Test processing response with no middleware."""
        pipeline = MiddlewarePipeline()
        response = {"status": 200, "body": "OK"}

        result = await pipeline.process_response(response)

        assert result == response

    @pytest.mark.asyncio
    async def test_process_response_reverse_order(self) -> None:
        """Test that responses are processed in reverse middleware order."""
        execution_order: list[str] = []

        class TrackingMiddleware1(AxumMiddleware):
            async def process_request(self, request_data: dict) -> dict | None:
                return request_data

            async def process_response(self, response_data: dict) -> dict:
                execution_order.append("mw1")
                return response_data

        class TrackingMiddleware2(AxumMiddleware):
            async def process_request(self, request_data: dict) -> dict | None:
                return request_data

            async def process_response(self, response_data: dict) -> dict:
                execution_order.append("mw2")
                return response_data

        pipeline = MiddlewarePipeline([TrackingMiddleware1(), TrackingMiddleware2()])
        response = {"status": 200}

        await pipeline.process_response(response)

        # Should be in reverse order
        assert execution_order == ["mw2", "mw1"]

    @pytest.mark.asyncio
    async def test_full_request_response_cycle(self) -> None:
        """Test full request-response cycle through pipeline."""
        pipeline = MiddlewarePipeline(
            [
                RequestLoggingMiddleware(),
                AuthenticationMiddleware(),
                RateLimitMiddleware(),
            ]
        )

        request = {
            "method": "POST",
            "url": "/graphql",
            "headers": {"Authorization": "Bearer token"},
            "client_ip": "192.168.1.1",
        }
        processed_request = await pipeline.process_request(request)

        assert processed_request == request

        response = {"status": 200, "body": "OK"}
        processed_response = await pipeline.process_response(response)

        assert processed_response == response
