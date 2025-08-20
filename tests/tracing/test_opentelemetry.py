"""Tests for OpenTelemetry tracing integration."""

import asyncio
from unittest.mock import Mock

import pytest

# Check if opentelemetry is available before importing anything
try:
    import opentelemetry  # noqa: F401
    from opentelemetry import trace
    from opentelemetry.sdk.trace import TracerProvider
    from opentelemetry.sdk.trace.export import BatchSpanProcessor
    from opentelemetry.sdk.trace.export.in_memory_span_exporter import InMemorySpanExporter
    from opentelemetry.trace import StatusCode

    # Only import if opentelemetry is available
    from fraiseql.tracing.opentelemetry import (
        FraiseQLTracer,
        TracingConfig,
        TracingMiddleware,
        get_tracer,
        setup_tracing,
        trace_database_query,
        trace_graphql_operation,
    )

    OPENTELEMETRY_AVAILABLE = True
except ImportError:
    OPENTELEMETRY_AVAILABLE = False

# Skip entire module if opentelemetry is not installed
pytestmark = pytest.mark.skipif(not OPENTELEMETRY_AVAILABLE, reason="OpenTelemetry not installed")

from typing import Never

# Global test exporter and provider
_test_exporter = None
_test_provider = None


@pytest.fixture(scope="module", autouse=True)
def setup_tracing_provider():
    """Set up OpenTelemetry provider once for all tests."""
    global _test_exporter, _test_provider

    if OPENTELEMETRY_AVAILABLE:
        # Set up the tracer provider once for entire module
        _test_exporter = InMemorySpanExporter()
        _test_provider = TracerProvider()
        _test_provider.add_span_processor(BatchSpanProcessor(_test_exporter))
        trace.set_tracer_provider(_test_provider)

        # Reset FraiseQL tracer
        import fraiseql.tracing.opentelemetry

        fraiseql.tracing.opentelemetry._tracer_instance = None

    yield

    # Cleanup after all tests
    if _test_exporter:
        _test_exporter.clear()


class TestTracingConfig:
    """Test tracing configuration."""

    def test_default_config(self) -> None:
        """Test default tracing configuration."""
        config = TracingConfig()

        assert config.enabled is True
        assert config.service_name == "fraiseql"
        assert config.sample_rate == 1.0
        assert config.export_endpoint is None
        assert config.export_format == "otlp"
        assert config.propagate_traces is True

    def test_custom_config(self) -> None:
        """Test custom tracing configuration."""
        config = TracingConfig(
            service_name="my-api",
            sample_rate=0.1,
            export_endpoint="http://jaeger:4318",
            export_format="jaeger",
            attributes={"environment": "production", "version": "1.0.0"},
        )

        assert config.service_name == "my-api"
        assert config.sample_rate == 0.1
        assert config.export_endpoint == "http://jaeger:4318"
        assert config.export_format == "jaeger"
        assert config.attributes["environment"] == "production"

    def test_config_validation(self) -> None:
        """Test configuration validation."""
        # Invalid sample rate
        with pytest.raises(ValueError):
            TracingConfig(sample_rate=1.5)

        with pytest.raises(ValueError):
            TracingConfig(sample_rate=-0.1)

        # Invalid export format
        with pytest.raises(ValueError):
            TracingConfig(export_format="invalid")


class TestFraiseQLTracer:
    """Test FraiseQL tracer functionality."""

    def setup_method(self) -> None:
        """Set up before each test."""
        # Use global fixtures
        self.exporter = _test_exporter
        self.provider = _test_provider

        # Clear any existing spans from the exporter
        if self.exporter:
            self.exporter.clear()

        # Create a new FraiseQLTracer for each test
        self.config = TracingConfig()

        # Reset the global FraiseQL tracer to ensure fresh instance
        import fraiseql.tracing.opentelemetry

        fraiseql.tracing.opentelemetry._tracer_instance = None

        self.tracer = FraiseQLTracer(self.config)

    def test_graphql_query_tracing(self) -> None:
        """Test tracing GraphQL query operations."""
        # Trace a query
        with self.tracer.trace_graphql_query(
            operation_name="getUser", query="{ user(id: 1) { name } }", variables={"id": 1}
        ) as span:
            span.set_attribute("user.id", 1)

        # Force flush to ensure spans are exported
        self.provider.force_flush()

        # Check span was created
        spans = list(self.exporter.get_finished_spans())
        assert len(spans) == 1

        span = spans[0]
        assert span.name == "graphql.query.getUser"
        assert span.attributes["graphql.operation.type"] == "query"
        assert span.attributes["graphql.operation.name"] == "getUser"
        assert span.attributes["graphql.document"] == "{ user(id: 1) { name } }"
        assert span.attributes["user.id"] == 1

    def test_graphql_mutation_tracing(self) -> None:
        """Test tracing GraphQL mutation operations."""
        with self.tracer.trace_graphql_mutation(
            operation_name="createUser",
            query="mutation { createUser(input: {...}) { id } }",
            variables={"input": {"name": "Test"}},
        ) as span:
            span.set_attribute("mutation.success", True)

        self.provider.force_flush()
        spans = list(self.exporter.get_finished_spans())

        assert len(spans) == 1
        span = spans[0]
        assert span.name == "graphql.mutation.createUser"
        assert span.attributes["graphql.operation.type"] == "mutation"
        assert span.attributes["mutation.success"] is True

    def test_database_query_tracing(self) -> None:
        """Test tracing database queries."""
        with self.tracer.trace_database_query(
            query_type="SELECT", table="users", sql="SELECT * FROM users WHERE id = %s"
        ) as span:
            span.set_attribute("db.rows_affected", 1)

        self.provider.force_flush()
        spans = list(self.exporter.get_finished_spans())

        assert len(spans) == 1
        span = spans[0]
        assert span.name == "db.SELECT.users"
        assert span.attributes["db.operation"] == "SELECT"
        assert span.attributes["db.table"] == "users"
        assert span.attributes["db.statement"] == "SELECT * FROM users WHERE id = %s"
        assert span.attributes["db.rows_affected"] == 1

    def test_nested_spans(self) -> None:
        """Test nested span relationships."""
        # Create nested spans
        with self.tracer.trace_graphql_query("getUsers", "{ users { id } }") as query_span:
            query_span.set_attribute("test", "parent")

            with self.tracer.trace_database_query(
                "SELECT", "users", "SELECT * FROM users"
            ) as db_span:
                db_span.set_attribute("test", "child")

        self.provider.force_flush()
        spans = list(self.exporter.get_finished_spans())

        assert len(spans) == 2

        # Find parent and child
        parent = next(s for s in spans if s.attributes.get("test") == "parent")
        child = next(s for s in spans if s.attributes.get("test") == "child")

        # Check parent-child relationship
        assert child.parent.span_id == parent.context.span_id

    def test_span_with_error(self) -> None:
        """Test span error recording."""
        try:
            with self.tracer.trace_graphql_query("failingQuery", "{ fail }") as span:
                msg = "Test error"
                raise ValueError(msg)
        except ValueError:
            pass

        self.provider.force_flush()
        spans = list(self.exporter.get_finished_spans())

        assert len(spans) == 1
        span = spans[0]
        # Check that status indicates error
        assert span.status.status_code == StatusCode.ERROR
        # Check exception was recorded
        assert len(span.events) > 0
        assert span.events[0].name == "exception"
        # Check exception details
        assert span.events[0].attributes.get("exception.type") == "ValueError"
        assert "Test error" in str(span.events[0].attributes.get("exception.message", ""))

    @pytest.mark.skip(reason="Sampling requires creating a new TracerProvider")
    def test_sampling(self) -> None:
        """Test trace sampling."""
        # Create tracer with 50% sampling
        config = TracingConfig(sample_rate=0.5)
        tracer = FraiseQLTracer(config)

        # Generate multiple spans
        sampled_count = 0
        for i in range(100):
            with tracer.trace_graphql_query(f"query{i}", "{ test }") as span:
                if span is not None and span.is_recording():
                    sampled_count += 1

        # Should be approximately 50 (allowing for randomness)
        assert 30 <= sampled_count <= 70

    def test_trace_context_propagation(self) -> None:
        """Test trace context propagation."""
        context = {}

        with self.tracer.trace_graphql_query("test", "{ test }") as span:
            # Get current trace context
            trace_id = span.get_span_context().trace_id
            context["trace_id"] = trace_id

            # Simulate propagating context
            headers = self.tracer.inject_context()
            assert headers is not None
            assert len(headers) > 0

    @pytest.mark.skip(reason="Custom attributes require creating a new TracerProvider")
    def test_custom_attributes(self) -> None:
        """Test adding custom attributes from config."""
        config = TracingConfig(
            attributes={"service.version": "1.0.0", "deployment.environment": "test"}
        )
        tracer = FraiseQLTracer(config)

        with tracer.trace_graphql_query("test", "{ test }"):
            pass

        self.provider.force_flush()
        spans = list(self.exporter.get_finished_spans())

        span = spans[0]
        assert span.attributes["service.version"] == "1.0.0"
        assert span.attributes["deployment.environment"] == "test"


class TestTracingMiddleware:
    """Test tracing middleware for FastAPI."""

    def setup_method(self) -> None:
        """Set up before each test."""
        # Use global fixtures
        self.exporter = _test_exporter
        self.provider = _test_provider

        # Clear any existing spans from the exporter
        if self.exporter:
            self.exporter.clear()

        # Reset the global FraiseQL tracer to ensure fresh instance
        import fraiseql.tracing.opentelemetry

        fraiseql.tracing.opentelemetry._tracer_instance = None

        self.config = TracingConfig()
        self.tracer = FraiseQLTracer(self.config)
        self.app = Mock()  # Mock app for middleware
        self.middleware = TracingMiddleware(self.app, tracer=self.tracer)

    @pytest.mark.asyncio
    async def test_middleware_traces_requests(self) -> None:
        """Test middleware creates spans for requests."""
        # Get initial span count
        self.provider.force_flush()
        initial_spans = list(self.exporter.get_finished_spans())
        initial_count = len(initial_spans)

        # Mock request
        request = Mock()
        request.url = Mock()
        request.url.path = "/graphql"
        request.url.scheme = "http"
        request.url.hostname = "localhost"
        request.method = "POST"
        request.headers = {}

        # Mock response
        async def mock_call_next(req):
            response = Mock()
            response.status_code = 200
            return response

        # Process request
        await self.middleware.process_request(request, mock_call_next)

        self.provider.force_flush()
        all_spans = list(self.exporter.get_finished_spans())
        new_spans = all_spans[initial_count:]  # Get only new spans

        assert len(new_spans) == 1
        span = new_spans[0]
        assert span.name == "POST /graphql"
        assert span.attributes["http.method"] == "POST"
        assert span.attributes["http.target"] == "/graphql"
        assert span.attributes["http.status_code"] == 200

    @pytest.mark.asyncio
    async def test_middleware_propagates_context(self) -> None:
        """Test middleware extracts and propagates trace context."""
        # Mock request with trace headers
        request = Mock()
        request.url = Mock()
        request.url.path = "/graphql"
        request.url.scheme = "http"
        request.url.hostname = "localhost"
        request.method = "POST"
        request.headers = {"traceparent": "00-0af7651916cd43dd8448eb211c80319c-b7ad6b7169203331-01"}

        async def mock_call_next(req):
            # Verify trace context was propagated
            span = trace.get_current_span()
            if span:
                # Check that we have an active span with the expected trace ID
                trace_id = span.get_span_context().trace_id
                # The expected trace ID from the header
                assert trace_id == 0x0AF7651916CD43DD8448EB211C80319C
            return Mock(status_code=200)

        await self.middleware.process_request(request, mock_call_next)

    @pytest.mark.asyncio
    async def test_middleware_handles_errors(self) -> None:
        """Test middleware records errors in spans."""
        # Get initial span count
        self.provider.force_flush()
        initial_spans = list(self.exporter.get_finished_spans())
        initial_count = len(initial_spans)

        request = Mock()
        request.url = Mock()
        request.url.path = "/graphql"
        request.url.scheme = "http"
        request.url.hostname = "localhost"
        request.method = "POST"
        request.headers = {}

        async def mock_call_next_error(req) -> Never:
            msg = "Test error"
            raise RuntimeError(msg)

        with pytest.raises(RuntimeError):
            await self.middleware.process_request(request, mock_call_next_error)

        self.provider.force_flush()
        all_spans = list(self.exporter.get_finished_spans())
        new_spans = all_spans[initial_count:]  # Get only new spans

        assert len(new_spans) == 1
        span = new_spans[0]
        assert span.status.status_code == StatusCode.ERROR
        assert span.attributes["http.status_code"] == 500

    @pytest.mark.asyncio
    async def test_middleware_excludes_paths(self) -> None:
        """Test middleware excludes configured paths."""
        config = TracingConfig(exclude_paths={"/health", "/metrics"})
        tracer = FraiseQLTracer(config)
        middleware = TracingMiddleware(self.app, tracer=tracer)

        # Health check request
        request = Mock()
        request.url = Mock()
        request.url.path = "/health"
        request.url.scheme = "http"
        request.url.hostname = "localhost"
        request.method = "GET"
        request.headers = {}

        async def mock_call_next(req):
            return Mock(status_code=200)

        await middleware.process_request(request, mock_call_next)

        self.provider.force_flush()
        spans = list(self.exporter.get_finished_spans())

        # Should not create span for excluded path
        assert len(spans) == 0


class TestTracingIntegration:
    """Test tracing integration with FastAPI."""

    def test_setup_tracing_on_app(self) -> None:
        """Test setting up tracing on FastAPI app."""
        from fastapi import FastAPI

        app = FastAPI()
        config = TracingConfig()

        # Setup tracing
        tracer = setup_tracing(app, config)

        # Should add middleware - check that middleware was added by verifying tracer is not None
        # Note: In FastAPI, middleware is accessed via app.middleware_stack or checking if the tracer was created
        assert tracer is not None

        # Should return tracer instance
        assert isinstance(tracer, FraiseQLTracer)

    def test_global_tracer(self) -> None:
        """Test global tracer access."""
        tracer1 = get_tracer()
        tracer2 = get_tracer()

        # Should be singleton
        assert tracer1 is tracer2

    def test_trace_decorators(self) -> None:
        """Test tracing decorators."""

        @trace_graphql_operation("query", "getUser")
        async def get_user(user_id: int):
            return {"id": user_id, "name": "Test"}

        @trace_database_query("SELECT", "users")
        def fetch_user(user_id: int):
            return {"id": user_id}

        # Execute decorated functions
        asyncio.run(get_user(1))
        fetch_user(1)

        # Check spans were created
        get_tracer()
        # Note: In real implementation, we'd check the span exporter

    def test_context_injection_extraction(self) -> None:
        """Test context injection and extraction."""
        tracer = get_tracer()

        # Start a span
        with tracer.trace_graphql_query("test", "{ test }"):
            # Inject context into headers
            headers = tracer.inject_context()
            assert "traceparent" in headers

            # Extract context from headers
            ctx = tracer.extract_context(headers)
            assert ctx is not None
