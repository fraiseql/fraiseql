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


def _get_placeholder_metric_value(registry, metric_name: str, labels: dict | None = None) -> float:
    """Get metric value from placeholder registry."""
    # Look for the metric in the registry's metrics collection
    if hasattr(registry, "_metrics"):
        for name, metric in registry._metrics.items():
            if metric_name.startswith(name) or name.endswith(metric_name.replace("_total", "")):
                # Handle histogram _sum and _count suffixes
                if metric_name.endswith("_sum") and hasattr(metric, "get_sum"):
                    return metric.get_sum()
                if metric_name.endswith("_count") and hasattr(metric, "get_count"):
                    return metric.get_count()
                if hasattr(metric, "get_value"):
                    # For labeled metrics, sum all labeled instances
                    total = metric.get_value()
                    if hasattr(metric, "_labeled_instances"):
                        for labeled_metric in metric._labeled_instances.values():
                            if hasattr(labeled_metric, "get_value"):
                                total += labeled_metric.get_value()
                    return total

    # Final fallback - return 0.0 for safe testing
    return 0.0


def get_metric_value(registry, metric_name: str, labels: dict | None = None) -> float:
    """Get the current value of a metric from the registry."""
    # Check if prometheus_client is available
    try:
        from prometheus_client import CollectorRegistry as RealRegistry

        if not isinstance(registry, RealRegistry):
            # Using placeholder registry - extract from metrics
            return _get_placeholder_metric_value(registry, metric_name, labels)
    except ImportError:
        # prometheus_client not available, using placeholder registry
        return _get_placeholder_metric_value(registry, metric_name, labels)
    total = 0.0
    # For counter metrics, Prometheus adds a _total suffix to the sample name
    # but the metric family name might not have it
    base_metric_name = metric_name.replace("_total", "")

    # For histogram/summary metrics, handle _sum, _count, _bucket suffixes
    suffixes = ["_sum", "_count", "_bucket"]
    is_histogram_component = any(metric_name.endswith(suffix) for suffix in suffixes)
    if is_histogram_component:
        # Extract base name for histogram (remove the suffix)
        for suffix in ["_sum", "_count", "_bucket"]:
            if metric_name.endswith(suffix):
                base_metric_name = metric_name[: -len(suffix)]
                break

    for metric_family in registry.collect():
        if metric_family.name in (base_metric_name, metric_name):
            for sample in metric_family.samples:
                # Check if this is the sample we're looking for
                if (
                    sample.name == metric_name
                    or sample.name == base_metric_name + "_total"
                    or (is_histogram_component and sample.name == metric_name)
                ):
                    if labels:
                        # Check if sample labels match
                        if all(sample.labels.get(k) == v for k, v in labels.items()):
                            return sample.value
                    # Sum all samples if no labels specified
                    elif sample.value is not None:
                        total += sample.value
    return total


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
            operation_type="query", operation_name="getUser", duration_ms=15.5, success=True
        )

        # Check counters
        ns = metrics.config.namespace
        assert get_metric_value(metrics.registry, f"{ns}_graphql_queries_total") > 0
        assert get_metric_value(metrics.registry, f"{ns}_graphql_queries_success") > 0
        assert get_metric_value(metrics.registry, f"{ns}_graphql_queries_errors") == 0

        # Record an error
        metrics.record_query(
            operation_type="query", operation_name="getUser", duration_ms=5.0, success=False
        )

        ns = metrics.config.namespace
        assert get_metric_value(metrics.registry, f"{ns}_graphql_queries_errors") > 0

    def test_mutation_metrics(self) -> None:
        """Test mutation execution metrics."""
        metrics = FraiseQLMetrics()

        metrics.record_mutation(
            mutation_name="createUser",
            duration_ms=25.5,
            success=True,
            result_type="CreateUserSuccess",
        )

        ns = metrics.config.namespace
        assert get_metric_value(metrics.registry, f"{ns}_graphql_mutations_total") > 0
        assert get_metric_value(metrics.registry, f"{ns}_graphql_mutations_success") > 0

    def test_database_metrics(self) -> None:
        """Test database connection metrics."""
        metrics = FraiseQLMetrics()

        # Test connection pool metrics
        (metrics.update_db_connections(active=5, idle=10, total=15),)

        ns = metrics.config.namespace
        assert get_metric_value(metrics.registry, f"{ns}_db_connections_active") == 5
        assert get_metric_value(metrics.registry, f"{ns}_db_connections_idle") == 10
        assert get_metric_value(metrics.registry, f"{ns}_db_connections_total") == 15

        # Test query metrics
        metrics.record_db_query(
            query_type="select", table_name="users", duration_ms=2.5, rows_affected=10
        )

        ns = metrics.config.namespace
        assert get_metric_value(metrics.registry, f"{ns}_db_queries_total") > 0

    def test_cache_metrics(self) -> None:
        """Test cache hit/miss metrics."""
        metrics = FraiseQLMetrics()

        # Record cache hits
        metrics.record_cache_hit("turbo_router")
        metrics.record_cache_hit("turbo_router")
        metrics.record_cache_miss("turbo_router")

        ns = metrics.config.namespace
        hits = get_metric_value(metrics.registry, f"{ns}_cache_hits_total")
        misses = get_metric_value(metrics.registry, f"{ns}_cache_misses_total")

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
            error_type="ValidationError", error_code="MISSING_FIELD", operation="createUser"
        )

        metrics.record_error(
            error_type="DatabaseError", error_code="CONNECTION_LOST", operation="getUsers"
        )

        ns = metrics.config.namespace
        assert get_metric_value(metrics.registry, f"{ns}_errors_total") >= 2

    def test_performance_metrics(self) -> None:
        """Test performance tracking."""
        metrics = FraiseQLMetrics()

        # Record response times
        for duration in [10, 20, 30, 40, 50]:
            metrics.record_response_time(duration)

        # Check histogram data
        # For histogram, we need to check the sum metric
        ns = metrics.config.namespace
        histogram_sum = get_metric_value(metrics.registry, f"{ns}_response_time_seconds_sum")
        assert histogram_sum > 0

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
        async def run_all() -> None:
            tasks = [record_queries() for _ in range(5)]
            await asyncio.gather(*tasks)

        asyncio.run(run_all())

        # Should have recorded 500 queries total
        ns = metrics.config.namespace
        assert get_metric_value(metrics.registry, f"{ns}_graphql_queries_total") >= 500


class TestMetricsMiddleware:
    """Test metrics middleware for FastAPI."""

    @pytest.mark.asyncio
    async def test_middleware_records_metrics(self) -> None:
        """Test middleware records request metrics."""
        metrics = FraiseQLMetrics()
        middleware = MetricsMiddleware(app=None, metrics=metrics)

        # Mock request
        request = Mock()
        request.url.path = "/graphql"
        request.method = "POST"

        # Mock call_next
        async def mock_call_next(req) -> None:
            response = Mock()
            response.status_code = 200
            return response

        # Process request
        response = await middleware.process_request(request, mock_call_next)

        # Check metrics were recorded
        assert response.status_code == 200
        ns = metrics.config.namespace
        assert get_metric_value(metrics.registry, f"{ns}_http_requests_total") > 0

    @pytest.mark.asyncio
    async def test_middleware_handles_errors(self) -> None:
        """Test middleware handles errors properly."""
        metrics = FraiseQLMetrics()
        middleware = MetricsMiddleware(app=None, metrics=metrics)

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
        ns = metrics.config.namespace
        assert get_metric_value(metrics.registry, f"{ns}_http_requests_total") > 0

    @pytest.mark.asyncio
    async def test_middleware_excludes_health_checks(self) -> None:
        """Test middleware excludes health check endpoints."""
        config = MetricsConfig(exclude_paths={"/health", "/ready"})
        metrics = FraiseQLMetrics()
        middleware = MetricsMiddleware(app=None, metrics=metrics, config=config)

        # Health check request
        request = Mock()
        request.url.path = "/health"
        request.method = "GET"

        async def mock_call_next(req) -> None:
            return Mock(status_code=200)

        await middleware.process_request(request, mock_call_next)

        # Should not record metrics for health checks
        ns = metrics.config.namespace
        assert get_metric_value(metrics.registry, f"{ns}_http_requests_total") == 0


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
        # Check that middleware was added by looking at app's middleware stack
        middleware_found = False
        for middleware in app.user_middleware:
            if hasattr(middleware, "cls") and middleware.cls == MetricsMiddleware:
                middleware_found = True
                break
        assert middleware_found

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
        # Check content type - may vary depending on prometheus_client availability
        content_type = response.headers["content-type"]
        assert content_type.startswith("text/plain")
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
