"""Tests for OpenTelemetry tracing integration."""

import asyncio
from unittest.mock import Mock

import pytest

# Skip tests if opentelemetry is not installed
pytest.importorskip("opentelemetry")

from typing import Never

from opentelemetry import trace
from opentelemetry.sdk.trace import TracerProvider
from opentelemetry.sdk.trace.export import BatchSpanProcessor
from opentelemetry.sdk.trace.export.in_memory_span_exporter import InMemorySpanExporter

from fraiseql.tracing.opentelemetry import (
    FraiseQLTracer,
    TracingConfig,
    TracingMiddleware,
    get_tracer,
    setup_tracing,
    trace_database_query,
    trace_graphql_operation,
)


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
        """Set up test tracer with in-memory exporter."""
        self.exporter = InMemorySpanExporter()
        self.provider = TracerProvider()
        self.provider.add_span_processor(BatchSpanProcessor(self.exporter))
        trace.set_tracer_provider(self.provider)

        self.config = TracingConfig()
        self.tracer = FraiseQLTracer(self.config)

    def test_graphql_query_tracing(self) -> None:
        """Test tracing GraphQL query operations."""
        # Trace a query
        with self.tracer.trace_graphql_query(
            operation_name="getUser",
            query="{ user(id: 1) { name } }",
            variables={"id": 1},
        ) as span:
            span.set_attribute("user.id", 1)

        # Force flush to ensure spans are exported
        self.provider.force_flush()

        # Check span was created
        spans = self.exporter.get_finished_spans()
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
        spans = self.exporter.get_finished_spans()

        assert len(spans) == 1
        span = spans[0]
        assert span.name == "graphql.mutation.createUser"
        assert span.attributes["graphql.operation.type"] == "mutation"
        assert span.attributes["mutation.success"] is True

    def test_database_query_tracing(self) -> None:
        """Test tracing database queries."""
        with self.tracer.trace_database_query(
            query_type="SELECT",
            table="users",
            sql="SELECT * FROM users WHERE id = %s",
        ) as span:
            span.set_attribute("db.rows_affected", 1)

        self.provider.force_flush()
        spans = self.exporter.get_finished_spans()

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
                "SELECT", "users", "SELECT * FROM users",
            ) as db_span:
                db_span.set_attribute("test", "child")

        self.provider.force_flush()
        spans = self.exporter.get_finished_spans()

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
        spans = self.exporter.get_finished_spans()

        assert len(spans) == 1
        span = spans[0]
        assert span.status.is_error
        assert "ValueError" in span.events[0].name
        assert "Test error" in str(span.events[0].attributes["exception.message"])

    def test_sampling(self) -> None:
        """Test trace sampling."""
        # Create tracer with 50% sampling
        config = TracingConfig(sample_rate=0.5)
        tracer = FraiseQLTracer(config)

        # Generate multiple spans
        sampled_count = 0
        for i in range(100):
            with tracer.trace_graphql_query(f"query{i}", "{ test }") as span:
                if span.is_recording():
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

    def test_custom_attributes(self) -> None:
        """Test adding custom attributes from config."""
        config = TracingConfig(
            attributes={"service.version": "1.0.0", "deployment.environment": "test"},
        )
        tracer = FraiseQLTracer(config)

        with tracer.trace_graphql_query("test", "{ test }") as span:
            pass

        self.provider.force_flush()
        spans = self.exporter.get_finished_spans()

        span = spans[0]
        assert span.attributes["service.version"] == "1.0.0"
        assert span.attributes["deployment.environment"] == "test"


class TestTracingMiddleware:
    """Test tracing middleware for FastAPI."""

    def setup_method(self) -> None:
        """Set up test environment."""
        self.exporter = InMemorySpanExporter()
        self.provider = TracerProvider()
        self.provider.add_span_processor(BatchSpanProcessor(self.exporter))
        trace.set_tracer_provider(self.provider)

        self.config = TracingConfig()
        self.tracer = FraiseQLTracer(self.config)
        self.middleware = TracingMiddleware(tracer=self.tracer)

    @pytest.mark.asyncio
    async def test_middleware_traces_requests(self) -> None:
        """Test middleware creates spans for requests."""
        # Mock request
        request = Mock()
        request.url.path = "/graphql"
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
        spans = self.exporter.get_finished_spans()

        assert len(spans) == 1
        span = spans[0]
        assert span.name == "POST /graphql"
        assert span.attributes["http.method"] == "POST"
        assert span.attributes["http.target"] == "/graphql"
        assert span.attributes["http.status_code"] == 200

    @pytest.mark.asyncio
    async def test_middleware_propagates_context(self) -> None:
        """Test middleware extracts and propagates trace context."""
        # Mock request with trace headers
        request = Mock()
        request.url.path = "/graphql"
        request.method = "POST"
        request.headers = {
            "traceparent": "00-0af7651916cd43dd8448eb211c80319c-b7ad6b7169203331-01",
        }

        captured_trace_id = None

        async def mock_call_next(req):
            # Capture current trace context
            span = trace.get_current_span()
            span.get_span_context().trace_id
            return Mock(status_code=200)

        await self.middleware.process_request(request, mock_call_next)

        # Should extract trace ID from header
        assert captured_trace_id == 0x0AF7651916CD43DD8448EB211C80319C

    @pytest.mark.asyncio
    async def test_middleware_handles_errors(self) -> None:
        """Test middleware records errors in spans."""
        request = Mock()
        request.url.path = "/graphql"
        request.method = "POST"
        request.headers = {}

        async def mock_call_next_error(req) -> Never:
            msg = "Test error"
            raise RuntimeError(msg)

        with pytest.raises(RuntimeError):
            await self.middleware.process_request(request, mock_call_next_error)

        self.provider.force_flush()
        spans = self.exporter.get_finished_spans()

        assert len(spans) == 1
        span = spans[0]
        assert span.status.is_error
        assert span.attributes["http.status_code"] == 500

    @pytest.mark.asyncio
    async def test_middleware_excludes_paths(self) -> None:
        """Test middleware excludes configured paths."""
        config = TracingConfig(exclude_paths={"/health", "/metrics"})
        tracer = FraiseQLTracer(config)
        middleware = TracingMiddleware(tracer=tracer)

        # Health check request
        request = Mock()
        request.url.path = "/health"
        request.method = "GET"
        request.headers = {}

        async def mock_call_next(req):
            return Mock(status_code=200)

        await middleware.process_request(request, mock_call_next)

        self.provider.force_flush()
        spans = self.exporter.get_finished_spans()

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

        # Should add middleware
        assert any(hasattr(m, "tracer") for m in app.middleware)

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
