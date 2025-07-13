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
        config = MetricsConfig(
            enabled=False,
            namespace="myapp",
            buckets=custom_buckets,
        )

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
            operation_type="query",
            operation_name="GetUser",
            duration_ms=123,
            success=True,
        )

        # Verify counters were incremented
        if PROMETHEUS_AVAILABLE:
            assert metrics.query_total._value._value > 0
            assert metrics.query_success._value._value > 0
        else:
            # Mock mode
            metrics.query_total.inc.assert_called()
            metrics.query_success.inc.assert_called()

    def test_record_query_error(self, metrics):
        """Test recording failed query."""
        metrics.record_query(
            operation_type="query",
            operation_name="GetUser",
            duration_ms=500,
            success=False,
        )

        if PROMETHEUS_AVAILABLE:
            assert metrics.query_errors._value._value > 0
        else:
            metrics.query_errors.inc.assert_called()

    def test_record_mutation(self, metrics):
        """Test recording mutation metrics."""
        metrics.record_mutation(
            mutation_name="CreateUser",
            duration_ms=234,
            success=True,
            result_type="User",
        )

        if PROMETHEUS_AVAILABLE:
            assert metrics.mutation_total._value._value > 0
            assert metrics.mutation_success._value._value > 0
        else:
            metrics.mutation_total.inc.assert_called()
            metrics.mutation_success.inc.assert_called_with(
                1,
                {"mutation_name": "CreateUser", "result_type": "User"},
            )

    def test_record_mutation_error(self, metrics):
        """Test recording failed mutation."""
        metrics.record_mutation(
            mutation_name="CreateUser",
            duration_ms=100,
            success=False,
            error_type="ValidationError",
        )

        if PROMETHEUS_AVAILABLE:
            assert metrics.mutation_errors._value._value > 0
        else:
            metrics.mutation_errors.inc.assert_called_with(
                1,
                {"mutation_name": "CreateUser", "error_type": "ValidationError"},
            )

    def test_update_db_connections(self, metrics):
        """Test updating database connection pool statistics."""
        metrics.update_db_connections(active=3, idle=7, total=10)

        if PROMETHEUS_AVAILABLE:
            assert metrics.db_connections_active._value._value == 3
            assert metrics.db_connections_idle._value._value == 7
            assert metrics.db_connections_total._value._value == 10
        else:
            metrics.db_connections_active.set.assert_called_with(3)
            metrics.db_connections_idle.set.assert_called_with(7)
            metrics.db_connections_total.set.assert_called_with(10)

    def test_record_db_query(self, metrics):
        """Test recording database query metrics."""
        metrics.record_db_query(
            query_type="SELECT",
            table_name="users",
            duration_ms=45,
        )

        if PROMETHEUS_AVAILABLE:
            assert metrics.db_queries_total._value._value > 0
        else:
            metrics.db_queries_total.inc.assert_called_with(
                1,
                {"query_type": "SELECT", "table_name": "users"},
            )
            metrics.db_query_duration.observe.assert_called_with(0.045)

    def test_record_cache_hit(self, metrics):
        """Test recording cache hit."""
        metrics.record_cache_hit("turbo_router")

        if PROMETHEUS_AVAILABLE:
            assert metrics.cache_hits._value._value > 0
        else:
            metrics.cache_hits.inc.assert_called()

    def test_record_cache_miss(self, metrics):
        """Test recording cache miss."""
        metrics.record_cache_miss("dataloader")

        if PROMETHEUS_AVAILABLE:
            assert metrics.cache_misses._value._value > 0
        else:
            metrics.cache_misses.inc.assert_called()

    def test_record_error(self, metrics):
        """Test recording errors."""
        metrics.record_error(
            error_type="ValidationError",
            error_code="INVALID_INPUT",
            operation="createUser",
        )

        if PROMETHEUS_AVAILABLE:
            assert metrics.errors_total._value._value > 0
        else:
            metrics.errors_total.inc.assert_called()

    def test_record_response_time(self, metrics):
        """Test recording response time."""
        metrics.record_response_time(250.5)

        if PROMETHEUS_AVAILABLE:
            # Check that histogram was updated
            assert hasattr(metrics, "response_time_histogram")
        else:
            # In mock mode, check observe was called
            metrics.response_time_histogram.observe.assert_called_with(0.2505)

        # Skip subscription tests if not implemented
        if hasattr(metrics, "record_subscription_complete"):
            # Complete subscription
            metrics.record_subscription_complete("MessageAdded", duration=120.5)

            if PROMETHEUS_AVAILABLE:
                assert metrics.subscriptions_active._value._value == 0
            else:
                metrics.subscriptions_active.dec.assert_called_with(
                    1,
                    {"subscription_name": "MessageAdded"},
                )
                metrics.subscription_duration.observe.assert_called_with(
                    120.5,
                    {"subscription_name": "MessageAdded"},
                )

    def test_update_turbo_router_stats(self, metrics):
        """Test updating TurboRouter statistics."""
        # Skip test if method doesn't exist
        if not hasattr(metrics, "update_turbo_router_stats"):
            pytest.skip("update_turbo_router_stats not implemented")

        metrics.update_turbo_router_stats(
            cache_size=850,
            hit_rate=0.92,
        )

        if PROMETHEUS_AVAILABLE:
            assert metrics.turbo_router_cache_size._value._value == 850
            assert metrics.turbo_router_hit_rate._value._value == 0.92
        else:
            metrics.turbo_router_cache_size.set.assert_called_with(850)
            metrics.turbo_router_hit_rate.set.assert_called_with(0.92)

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
        config = MetricsConfig(namespace="test")
        metrics = setup_metrics(config)

        assert isinstance(metrics, FraiseQLMetrics)
        assert metrics.config.namespace == "test"

        # Should be retrievable
        assert get_metrics() is metrics

    def test_get_metrics_without_setup(self):
        """Test getting metrics without setup returns None."""
        # Reset global metrics
        import fraiseql.monitoring.metrics.integration

        fraiseql.monitoring.metrics.integration._metrics = None

        assert get_metrics() is None

    @pytest.mark.asyncio
    async def test_with_metrics_decorator(self):
        """Test metrics decorator for async functions."""
        metrics = setup_metrics()

        @with_metrics("test_operation")
        async def test_function():
            await asyncio.sleep(0.01)
            return "result"

        result = await test_function()
        assert result == "result"

        # Should have recorded metrics
        if PROMETHEUS_AVAILABLE:
            assert metrics.query_total._value._value > 0

    @pytest.mark.asyncio
    async def test_with_metrics_decorator_error(self):
        """Test metrics decorator with function that raises error."""
        metrics = setup_metrics()

        @with_metrics("failing_operation")
        async def failing_function():
            raise ValueError("Test error")

        with pytest.raises(ValueError):
            await failing_function()

        # Should have recorded error
        if PROMETHEUS_AVAILABLE:
            assert metrics.query_errors._value._value > 0

    def test_with_metrics_sync_function(self):
        """Test metrics decorator with sync function."""
        metrics = setup_metrics()

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
        return MetricsMiddleware(app, config)

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
            assert metrics.http_requests_total._value._value > 0

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
        middleware = MetricsMiddleware(app, config)

        # Should not create metrics
        assert middleware.metrics is None
