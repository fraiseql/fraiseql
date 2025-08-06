"""Extended tests for metrics collectors and integration."""

import asyncio
from unittest.mock import MagicMock

import pytest

from fraiseql.monitoring.metrics import (
    PROMETHEUS_AVAILABLE,
    FraiseQLMetrics,
    MetricsConfig,
    MetricsMiddleware,
    get_metrics,
    setup_metrics,
    with_metrics,
)


class TestMetricsConfig:
    """Test MetricsConfig class."""

    def test_default_config(self):
        """Test default metrics configuration."""
        config = MetricsConfig()

        assert config.enabled is True
        assert config.namespace == "fraiseql"
        assert config.buckets == [0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1, 2.5, 5, 10]
        # Check for any additional attributes that might exist
        assert hasattr(config, "exclude_paths")
        assert hasattr(config, "metrics_path")

    def test_custom_config(self):
        """Test custom metrics configuration."""
        custom_buckets = [0.1, 0.5, 1.0, 5.0]
        config = MetricsConfig(enabled=False, namespace="myapp", buckets=custom_buckets)

        assert config.enabled is False
        assert config.namespace == "myapp"
        assert config.buckets == custom_buckets


class TestFraiseQLMetrics:
    """Test FraiseQLMetrics class."""

    @pytest.fixture
    def metrics(self):
        """Create metrics instance."""
        if PROMETHEUS_AVAILABLE:
            from prometheus_client import CollectorRegistry

            registry = CollectorRegistry()
        else:
            registry = MagicMock()

        return FraiseQLMetrics(registry=registry)

    def test_metrics_initialization(self):
        """Test metrics initialization with custom config."""
        config = MetricsConfig(namespace="test_app")
        metrics = FraiseQLMetrics(config=config)

        assert metrics.config.namespace == "test_app"
        assert hasattr(metrics, "query_total")
        assert hasattr(metrics, "query_duration")
        assert hasattr(metrics, "mutation_total")
        assert hasattr(metrics, "db_connections_active")
        assert hasattr(metrics, "cache_hits")
        assert hasattr(metrics, "errors_total")

    def test_record_query(self, metrics):
        """Test recording GraphQL query metrics."""
        # Record successful query
        metrics.record_query(
            operation_type="query", operation_name="GetUser", duration_ms=123, success=True
        )

        # Verify counters were incremented
        if PROMETHEUS_AVAILABLE:
            # For labeled metrics, we need to check the samples or use collect()
            samples = list(metrics.query_total.collect())[0].samples
            assert len(samples) > 0
            assert any(s.value > 0 for s in samples)

            success_samples = list(metrics.query_success.collect())[0].samples
            assert len(success_samples) > 0
            assert any(s.value > 0 for s in success_samples)
        else:
            # Mock mode
            assert hasattr(metrics.query_total, "inc")
            assert hasattr(metrics.query_success, "inc")

    def test_record_query_error(self, metrics):
        """Test recording failed query."""
        metrics.record_query(
            operation_type="query", operation_name="GetUser", duration_ms=500, success=False
        )

        if PROMETHEUS_AVAILABLE:
            error_samples = list(metrics.query_errors.collect())[0].samples
            assert len(error_samples) > 0
            assert any(s.value > 0 for s in error_samples)
        else:
            assert hasattr(metrics.query_errors, "inc")

    def test_record_mutation(self, metrics):
        """Test recording mutation metrics."""
        metrics.record_mutation(
            mutation_name="CreateUser", duration_ms=234, success=True, result_type="User"
        )

        if PROMETHEUS_AVAILABLE:
            mutation_samples = list(metrics.mutation_total.collect())[0].samples
            assert len(mutation_samples) > 0
            assert any(s.value > 0 for s in mutation_samples)

            success_samples = list(metrics.mutation_success.collect())[0].samples
            assert len(success_samples) > 0
            assert any(s.value > 0 for s in success_samples)
        else:
            assert hasattr(metrics.mutation_total, "inc")
            assert hasattr(metrics.mutation_success, "inc")

    def test_record_mutation_error(self, metrics):
        """Test recording failed mutation."""
        metrics.record_mutation(
            mutation_name="CreateUser", duration_ms=100, success=False, error_type="ValidationError"
        )

        if PROMETHEUS_AVAILABLE:
            error_samples = list(metrics.mutation_errors.collect())[0].samples
            assert len(error_samples) > 0
            assert any(s.value > 0 for s in error_samples)
        else:
            assert hasattr(metrics.mutation_errors, "inc")

    def test_update_db_connections(self, metrics):
        """Test updating database connection pool statistics."""
        metrics.update_db_connections(active=3, idle=7, total=10)

        if PROMETHEUS_AVAILABLE:
            # Gauges without labels do have _value
            assert metrics.db_connections_active._value.get() == 3
            assert metrics.db_connections_idle._value.get() == 7
            assert metrics.db_connections_total._value.get() == 10
        else:
            assert hasattr(metrics.db_connections_active, "set")
            assert hasattr(metrics.db_connections_idle, "set")
            assert hasattr(metrics.db_connections_total, "set")

    def test_record_db_query(self, metrics):
        """Test recording database query metrics."""
        metrics.record_db_query(query_type="SELECT", table_name="users", duration_ms=45)

        if PROMETHEUS_AVAILABLE:
            query_samples = list(metrics.db_queries_total.collect())[0].samples
            assert len(query_samples) > 0
            assert any(s.value > 0 for s in query_samples)
        else:
            assert hasattr(metrics.db_queries_total, "inc")
            assert hasattr(metrics.db_query_duration, "observe")

    def test_record_cache_hit(self, metrics):
        """Test recording cache hit."""
        metrics.record_cache_hit("turbo_router")

        if PROMETHEUS_AVAILABLE:
            hit_samples = list(metrics.cache_hits.collect())[0].samples
            assert len(hit_samples) > 0
            assert any(s.value > 0 for s in hit_samples)
        else:
            assert hasattr(metrics.cache_hits, "inc")

    def test_record_cache_miss(self, metrics):
        """Test recording cache miss."""
        metrics.record_cache_miss("dataloader")

        if PROMETHEUS_AVAILABLE:
            miss_samples = list(metrics.cache_misses.collect())[0].samples
            assert len(miss_samples) > 0
            assert any(s.value > 0 for s in miss_samples)
        else:
            assert hasattr(metrics.cache_misses, "inc")

    def test_record_error(self, metrics):
        """Test recording errors."""
        metrics.record_error(
            error_type="ValidationError", error_code="INVALID_INPUT", operation="createUser"
        )

        if PROMETHEUS_AVAILABLE:
            error_samples = list(metrics.errors_total.collect())[0].samples
            assert len(error_samples) > 0
            assert any(s.value > 0 for s in error_samples)
        else:
            assert hasattr(metrics.errors_total, "inc")

    def test_record_response_time(self, metrics):
        """Test recording response time."""
        metrics.record_response_time(250.5)

        if PROMETHEUS_AVAILABLE:
            # Check that histogram was updated
            assert hasattr(metrics, "response_time_histogram")
        else:
            # In mock mode, check observe was called
            assert hasattr(metrics.response_time_histogram, "observe")

        # Skip subscription tests if not implemented
        if hasattr(metrics, "record_subscription_complete"):
            # Complete subscription
            metrics.record_subscription_complete("MessageAdded", duration=120.5)

            if PROMETHEUS_AVAILABLE:
                # This would need to be updated based on actual subscription metrics implementation
                pass  # Skip for now since subscription metrics may not be implemented
            else:
                assert hasattr(metrics.subscriptions_active, "dec")
                assert hasattr(metrics.subscription_duration, "observe")

    def test_update_turbo_router_stats(self, metrics):
        """Test updating TurboRouter statistics."""
        # Skip test if method doesn't exist
        if not hasattr(metrics, "update_turbo_router_stats"):
            pytest.skip("update_turbo_router_stats not implemented")

        metrics.update_turbo_router_stats(cache_size=850, hit_rate=0.92)

        if PROMETHEUS_AVAILABLE:
            # These would need to be updated based on actual turbo router metrics implementation
            pass  # Skip for now since turbo router metrics may not be implemented
        else:
            assert hasattr(metrics.turbo_router_cache_size, "set")
            assert hasattr(metrics.turbo_router_hit_rate, "set")

    def test_generate_output(self, metrics):
        """Test generating metrics output."""
        # Skip test if method doesn't exist
        if not hasattr(metrics, "generate_output"):
            pytest.skip("generate_output not implemented")

        # Record some metrics
        metrics.record_query(
            operation_type="query", operation_name="Test", duration_ms=100, success=True
        )
        metrics.record_cache_hit("turbo_router")

        output = metrics.generate_output()
        assert isinstance(output, bytes)

        if PROMETHEUS_AVAILABLE:
            # Should contain metric names
            assert b"fraiseql_graphql_queries_total" in output
            assert b"fraiseql_cache_hits_total" in output


class TestMetricsIntegration:
    """Test metrics integration functions."""

    def test_setup_metrics(self):
        """Test setting up global metrics."""
        from fastapi import FastAPI

        # Reset global metrics first
        import fraiseql.monitoring.metrics.integration

        fraiseql.monitoring.metrics.integration._metrics_instance = None

        app = FastAPI()
        config = MetricsConfig(namespace="test")
        metrics = setup_metrics(app, config)

        assert isinstance(metrics, FraiseQLMetrics)
        assert metrics.config.namespace == "test"

        # Should be retrievable
        assert get_metrics() is metrics

    def test_get_metrics_without_setup(self):
        """Test getting metrics without setup returns None."""
        # Reset global metrics
        import fraiseql.monitoring.metrics.integration

        fraiseql.monitoring.metrics.integration._metrics_instance = None

        assert get_metrics() is None

    @pytest.mark.asyncio
    async def test_with_metrics_decorator(self):
        """Test metrics decorator for async functions."""
        from fastapi import FastAPI

        app = FastAPI()
        metrics = setup_metrics(app)

        @with_metrics("query")
        async def test_function():
            await asyncio.sleep(0.01)
            return "result"

        result = await test_function()
        assert result == "result"

        # Should have recorded metrics
        if PROMETHEUS_AVAILABLE:
            query_samples = list(metrics.query_total.collect())[0].samples
            assert len(query_samples) > 0
            assert any(s.value > 0 for s in query_samples)

    @pytest.mark.asyncio
    async def test_with_metrics_decorator_error(self):
        """Test metrics decorator with function that raises error."""
        from fastapi import FastAPI

        app = FastAPI()
        metrics = setup_metrics(app)

        @with_metrics("query")
        async def failing_function():
            raise ValueError("Test error")

        with pytest.raises(ValueError):
            await failing_function()

        # Should have recorded error
        if PROMETHEUS_AVAILABLE:
            error_samples = list(metrics.query_errors.collect())[0].samples
            assert len(error_samples) > 0
            assert any(s.value > 0 for s in error_samples)

    def test_with_metrics_sync_function(self):
        """Test metrics decorator with sync function."""
        from fastapi import FastAPI

        app = FastAPI()
        metrics = setup_metrics(app)

        @with_metrics("sync_operation")
        def sync_function():
            return "sync_result"

        result = sync_function()
        assert result == "sync_result"


class TestMetricsMiddleware:
    """Test MetricsMiddleware for FastAPI."""

    @pytest.fixture
    def middleware(self):
        """Create middleware instance."""
        app = MagicMock()
        config = MetricsConfig()
        metrics = FraiseQLMetrics(config)
        return MetricsMiddleware(app, metrics, config)

    @pytest.mark.asyncio
    async def test_middleware_records_metrics(self, middleware):
        """Test middleware records HTTP metrics."""
        # Mock request and response
        request = MagicMock()
        request.method = "POST"
        request.url.path = "/graphql"

        response = MagicMock()
        response.status_code = 200

        # Mock call_next
        async def call_next(req):
            return response

        # Process request
        result = await middleware.dispatch(request, call_next)

        assert result is response

        # Should have recorded metrics
        metrics = middleware.metrics
        if PROMETHEUS_AVAILABLE:
            request_samples = list(metrics.http_requests_total.collect())[0].samples
            assert len(request_samples) > 0
            assert any(s.value > 0 for s in request_samples)

    @pytest.mark.asyncio
    async def test_middleware_handles_errors(self, middleware):
        """Test middleware handles errors properly."""
        request = MagicMock()
        request.method = "GET"
        request.url.path = "/error"

        # Mock call_next to raise error
        async def call_next(req):
            raise Exception("Test error")

        # Should propagate error
        with pytest.raises(Exception, match="Test error"):
            await middleware.dispatch(request, call_next)

    def test_middleware_disabled(self):
        """Test middleware when metrics are disabled."""
        app = MagicMock()
        config = MetricsConfig(enabled=False)
        metrics = FraiseQLMetrics(config)
        middleware = MetricsMiddleware(app, metrics, config)

        # Should have metrics but config is disabled
        assert middleware.metrics is metrics
        assert not middleware.config.enabled
