"""
Integration tests for Phase 2.5: Pool Metrics.

Tests metrics collection and reporting:
- Query execution tracking
- Error tracking
- Success rate calculation
- Thread safety

Uses testcontainers for automatic PostgreSQL provisioning.
"""

import pytest
import pytest_asyncio

# Import database fixtures (provides postgres_url via testcontainers)
pytest_plugins = ["tests.fixtures.database.database_conftest"]


@pytest_asyncio.fixture
async def pool(postgres_url):
    """Create database pool for testing using testcontainers PostgreSQL."""
    from fraiseql._fraiseql_rs import DatabasePool

    # Use testcontainers URL
    async with DatabasePool(url=postgres_url, max_size=10, ssl_mode="disable") as pool:
        yield pool


class TestBasicMetrics:
    """Test basic metrics collection."""

    async def test_initial_metrics(self, pool):
        """Test initial metrics are zero."""
        metrics = pool.metrics()

        assert metrics["queries_executed"] == 0
        assert metrics["query_errors"] == 0
        assert metrics["health_checks"] == 0
        assert metrics["health_check_failures"] == 0
        assert metrics["query_success_rate"] == 1.0  # 100% when no queries

    async def test_query_execution_tracking(self, pool):
        """Test that successful queries are tracked."""
        # Execute a query
        await pool.execute_query("SELECT 1")

        metrics = pool.metrics()
        assert metrics["queries_executed"] == 1
        assert metrics["query_errors"] == 0
        assert metrics["query_success_rate"] == 1.0

    async def test_multiple_queries_tracked(self, pool):
        """Test that multiple queries increment counter."""
        # Execute 5 queries
        for _ in range(5):
            await pool.execute_query("SELECT 1")

        metrics = pool.metrics()
        assert metrics["queries_executed"] == 5
        assert metrics["query_errors"] == 0

    async def test_error_tracking(self, pool):
        """Test that query errors are tracked."""
        # Execute invalid query
        with pytest.raises(Exception):
            await pool.execute_query("INVALID SQL")

        metrics = pool.metrics()
        assert metrics["queries_executed"] == 0
        assert metrics["query_errors"] == 1

    async def test_mixed_success_and_errors(self, pool):
        """Test tracking both successful and failed queries."""
        # 3 successful queries
        await pool.execute_query("SELECT 1")
        await pool.execute_query("SELECT 2")
        await pool.execute_query("SELECT 3")

        # 1 failed query
        with pytest.raises(Exception):
            await pool.execute_query("INVALID SQL")

        metrics = pool.metrics()
        assert metrics["queries_executed"] == 3
        assert metrics["query_errors"] == 1
        assert metrics["query_success_rate"] == 0.75  # 3/4


class TestSuccessRate:
    """Test success rate calculations."""

    async def test_success_rate_all_successful(self, pool):
        """Test success rate is 100% with all successful queries."""
        for _ in range(10):
            await pool.execute_query("SELECT 1")

        metrics = pool.metrics()
        assert metrics["query_success_rate"] == 1.0

    async def test_success_rate_all_failed(self, pool):
        """Test success rate is 0% with all failed queries."""
        for _ in range(5):
            with pytest.raises(Exception):
                await pool.execute_query("INVALID SQL")

        metrics = pool.metrics()
        assert metrics["query_success_rate"] == 0.0

    async def test_success_rate_calculation(self, pool):
        """Test success rate calculation with mixed results."""
        # 7 successful
        for _ in range(7):
            await pool.execute_query("SELECT 1")

        # 3 failed
        for _ in range(3):
            with pytest.raises(Exception):
                await pool.execute_query("INVALID SQL")

        metrics = pool.metrics()
        assert metrics["query_success_rate"] == 0.7  # 7/10


class TestMetricsStructure:
    """Test metrics dictionary structure."""

    async def test_metrics_keys(self, pool):
        """Test that metrics dict has all expected keys."""
        metrics = pool.metrics()

        expected_keys = {
            "queries_executed",
            "query_errors",
            "health_checks",
            "health_check_failures",
            "query_success_rate",
            "health_check_success_rate",
        }

        assert set(metrics.keys()) == expected_keys

    async def test_metrics_types(self, pool):
        """Test that metric values have correct types."""
        await pool.execute_query("SELECT 1")
        metrics = pool.metrics()

        assert isinstance(metrics["queries_executed"], int)
        assert isinstance(metrics["query_errors"], int)
        assert isinstance(metrics["health_checks"], int)
        assert isinstance(metrics["health_check_failures"], int)
        assert isinstance(metrics["query_success_rate"], float)
        assert isinstance(metrics["health_check_success_rate"], float)


class TestConcurrentMetrics:
    """Test metrics with concurrent operations."""

    async def test_concurrent_queries_tracked(self, pool):
        """Test that concurrent queries are all tracked."""
        import asyncio

        # Run 20 queries concurrently
        await asyncio.gather(*[pool.execute_query("SELECT 1") for _ in range(20)])

        metrics = pool.metrics()
        assert metrics["queries_executed"] == 20
        assert metrics["query_errors"] == 0

    async def test_concurrent_mixed_operations(self, pool):
        """Test metrics with concurrent successes and failures."""
        import asyncio

        async def success_query():
            await pool.execute_query("SELECT 1")

        async def fail_query():
            try:
                await pool.execute_query("INVALID SQL")
            except:
                pass

        # 10 successes and 5 failures concurrently
        tasks = [success_query() for _ in range(10)] + [
            fail_query() for _ in range(5)
        ]
        await asyncio.gather(*tasks)

        metrics = pool.metrics()
        assert metrics["queries_executed"] == 10
        assert metrics["query_errors"] == 5
        assert abs(metrics["query_success_rate"] - 0.6667) < 0.01  # ~66.67%
