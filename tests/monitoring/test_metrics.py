"""Tests for Prometheus metrics integration."""

import asyncio
from typing import Never
from unittest.mock import Mock

import pytest

from fraiseql.monitoring.metrics import (
    FraiseQLMetrics,
    MetricsConfig,
    MetricsMiddleware,
    get_metrics,
    setup_metrics,
)


class TestFraiseQLMetrics:
    """Test metrics collection."""

    def test_metrics_singleton(self) -> None:
        """Test metrics instance is a singleton."""
        metrics1 = get_metrics()
        metrics2 = get_metrics()
        assert metrics1 is metrics2

    def test_query_metrics(self) -> None:
        """Test query execution metrics."""
        metrics = FraiseQLMetrics()

        # Record a query
        metrics.record_query(
            operation_type="query",
            operation_name="getUser",
            duration_ms=15.5,
            success=True,
        )

        # Check counters
        assert metrics.query_total._value.get() > 0
        assert metrics.query_success._value.get() > 0
        assert metrics.query_errors._value.get() == 0

        # Record an error
        metrics.record_query(
            operation_type="query",
            operation_name="getUser",
            duration_ms=5.0,
            success=False,
        )

        assert metrics.query_errors._value.get() > 0

    def test_mutation_metrics(self) -> None:
        """Test mutation execution metrics."""
        metrics = FraiseQLMetrics()

        metrics.record_mutation(
            mutation_name="createUser",
            duration_ms=25.5,
            success=True,
            result_type="CreateUserSuccess",
        )

        assert metrics.mutation_total._value.get() > 0
        assert metrics.mutation_success._value.get() > 0

    def test_database_metrics(self) -> None:
        """Test database connection metrics."""
        metrics = FraiseQLMetrics()

        # Test connection pool metrics
        metrics.update_db_connections(active=5, idle=10, total=15)

        assert metrics.db_connections_active._value.get() == 5
        assert metrics.db_connections_idle._value.get() == 10
        assert metrics.db_connections_total._value.get() == 15

        # Test query metrics
        metrics.record_db_query(
            query_type="select",
            table_name="users",
            duration_ms=2.5,
            rows_affected=10,
        )

        assert metrics.db_queries_total._value.get() > 0

    def test_cache_metrics(self) -> None:
        """Test cache hit/miss metrics."""
        metrics = FraiseQLMetrics()

        # Record cache hits
        metrics.record_cache_hit("turbo_router")
        metrics.record_cache_hit("turbo_router")
        metrics.record_cache_miss("turbo_router")

        hits = metrics.cache_hits._value.get()
        misses = metrics.cache_misses._value.get()

        assert hits == 2
        assert misses == 1

        # Cache hit rate should be 66.67%
        hit_rate = metrics.get_cache_hit_rate("turbo_router")
        assert hit_rate == pytest.approx(0.6667, rel=0.01)

    def test_error_metrics(self) -> None:
        """Test error tracking metrics."""
        metrics = FraiseQLMetrics()

        # Record different error types
        metrics.record_error(
            error_type="ValidationError",
            error_code="MISSING_FIELD",
            operation="createUser",
        )

        metrics.record_error(
            error_type="DatabaseError",
            error_code="CONNECTION_LOST",
            operation="getUsers",
        )

        assert metrics.errors_total._value.get() >= 2

    def test_performance_metrics(self) -> None:
        """Test performance tracking."""
        metrics = FraiseQLMetrics()

        # Record response times
        for duration in [10, 20, 30, 40, 50]:
            metrics.record_response_time(duration)

        # Check histogram data
        histogram_data = metrics.response_time_histogram._sum._value.get()
        assert histogram_data > 0

    def test_concurrent_metrics(self) -> None:
        """Test metrics under concurrent access."""
        metrics = FraiseQLMetrics()

        async def record_queries() -> None:
            for i in range(100):
                metrics.record_query(
                    operation_type="query",
                    operation_name=f"query{i}",
                    duration_ms=i * 0.1,
                    success=i % 10 != 0,  # 10% errors
                )
                await asyncio.sleep(0.001)

        # Run concurrent tasks
        loop = asyncio.new_event_loop()
        tasks = [record_queries() for _ in range(5)]
        loop.run_until_complete(asyncio.gather(*tasks))

        # Should have recorded 500 queries total
        assert metrics.query_total._value.get() >= 500


class TestMetricsMiddleware:
    """Test metrics middleware for FastAPI."""

    @pytest.mark.asyncio
    async def test_middleware_records_metrics(self) -> None:
        """Test middleware records request metrics."""
        metrics = FraiseQLMetrics()
        middleware = MetricsMiddleware(metrics=metrics)

        # Mock request
        request = Mock()
        request.url.path = "/graphql"
        request.method = "POST"

        # Mock call_next
        async def mock_call_next(req):
            response = Mock()
            response.status_code = 200
            return response

        # Process request
        response = await middleware.process_request(request, mock_call_next)

        # Check metrics were recorded
        assert response.status_code == 200
        assert metrics.http_requests_total._value.get() > 0

    @pytest.mark.asyncio
    async def test_middleware_handles_errors(self) -> None:
        """Test middleware handles errors properly."""
        metrics = FraiseQLMetrics()
        middleware = MetricsMiddleware(metrics=metrics)

        request = Mock()
        request.url.path = "/graphql"
        request.method = "POST"

        # Mock error
        async def mock_call_next_error(req) -> Never:
            msg = "Test error"
            raise RuntimeError(msg)

        # Should propagate error but record metrics
        with pytest.raises(RuntimeError):
            await middleware.process_request(request, mock_call_next_error)

        # Error metrics should be recorded
        assert metrics.http_requests_total._value.get() > 0

    @pytest.mark.asyncio
    async def test_middleware_excludes_health_checks(self) -> None:
        """Test middleware excludes health check endpoints."""
        config = MetricsConfig(exclude_paths={"/health", "/ready"})
        metrics = FraiseQLMetrics()
        middleware = MetricsMiddleware(metrics=metrics, config=config)

        # Health check request
        request = Mock()
        request.url.path = "/health"
        request.method = "GET"

        async def mock_call_next(req):
            return Mock(status_code=200)

        await middleware.process_request(request, mock_call_next)

        # Should not record metrics for health checks
        assert metrics.http_requests_total._value.get() == 0


class TestMetricsConfig:
    """Test metrics configuration."""

    def test_default_config(self) -> None:
        """Test default metrics configuration."""
        config = MetricsConfig()

        assert config.enabled is True
        assert config.namespace == "fraiseql"
        assert config.buckets == [0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1, 2.5, 5, 10]
        assert "/metrics" in config.exclude_paths

    def test_custom_config(self) -> None:
        """Test custom metrics configuration."""
        config = MetricsConfig(
            enabled=False,
            namespace="myapp",
            buckets=[0.1, 0.5, 1.0],
            labels={"environment": "production", "region": "us-east-1"},
        )

        assert config.enabled is False
        assert config.namespace == "myapp"
        assert len(config.buckets) == 3
        assert config.labels["environment"] == "production"

    def test_config_validation(self) -> None:
        """Test configuration validation."""
        # Should reject invalid histogram buckets
        with pytest.raises(ValueError):
            MetricsConfig(buckets=[1, 0.5, 0.1])  # Not monotonic

        # Should reject empty namespace
        with pytest.raises(ValueError):
            MetricsConfig(namespace="")


class TestMetricsSetup:
    """Test metrics setup and integration."""

    def test_setup_metrics_on_app(self) -> None:
        """Test setting up metrics on FastAPI app."""
        from fastapi import FastAPI

        app = FastAPI()
        config = MetricsConfig()

        # Setup metrics
        metrics = setup_metrics(app, config)

        # Should add middleware
        assert any(isinstance(m, MetricsMiddleware) for m in app.middleware)

        # Should add metrics endpoint
        assert any(route.path == "/metrics" for route in app.routes)

        # Should return metrics instance
        assert isinstance(metrics, FraiseQLMetrics)

    def test_metrics_endpoint(self) -> None:
        """Test Prometheus metrics endpoint."""
        from fastapi import FastAPI
        from fastapi.testclient import TestClient

        app = FastAPI()
        metrics = setup_metrics(app)

        # Record some metrics
        metrics.record_query("query", "getUsers", 10.5, True)

        # Test metrics endpoint
        client = TestClient(app)
        response = client.get("/metrics")

        assert response.status_code == 200
        assert response.headers["content-type"] == "text/plain; version=0.0.4; charset=utf-8"
        assert "fraiseql_graphql_queries_total" in response.text
        assert "fraiseql_graphql_query_duration_seconds" in response.text

    def test_custom_metrics_path(self) -> None:
        """Test custom metrics endpoint path."""
        from fastapi import FastAPI
        from fastapi.testclient import TestClient

        app = FastAPI()
        config = MetricsConfig(metrics_path="/custom-metrics")
        setup_metrics(app, config)

        client = TestClient(app)

        # Default path should not exist
        response = client.get("/metrics")
        assert response.status_code == 404

        # Custom path should work
        response = client.get("/custom-metrics")
        assert response.status_code == 200


class TestMetricsLabels:
    """Test metric labels and cardinality."""

    def test_operation_labels(self) -> None:
        """Test operation-specific labels."""
        metrics = FraiseQLMetrics()

        # Record queries with different labels
        metrics.record_query("query", "getUser", 10, True)
        metrics.record_query("query", "getUsers", 15, True)
        metrics.record_query("mutation", "createUser", 25, True)

        # Each combination should have its own counter
        # This is a simplified test - in reality we'd check Prometheus output
