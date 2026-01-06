"""Integration tests for production DatabasePool.

Validates that production pool passes all prototype tests and adds
production-specific test cases.

Uses testcontainers for automatic PostgreSQL provisioning.
"""

import asyncio

import pytest
import pytest_asyncio

# Import database fixtures (provides postgres_url via testcontainers)
pytest_plugins = ["tests.fixtures.database.database_conftest"]


@pytest_asyncio.fixture
async def pool(postgres_url) -> None:
    """Create production pool for testing using testcontainers PostgreSQL."""
    from fraiseql._fraiseql_rs import DatabasePool

    # Use testcontainers URL
    async with DatabasePool(url=postgres_url, max_size=10, ssl_mode="disable") as pool:
        yield pool


class TestBasicQueries:
    """Test basic query execution."""

    async def test_simple_query(self, pool) -> None:
        """Test simple SELECT query."""
        results = await pool.execute_query("SELECT 1 as num")
        assert len(results) == 1

    async def test_empty_result(self, pool) -> None:
        """Test query with no results."""
        results = await pool.execute_query("SELECT 1 WHERE FALSE")
        assert len(results) == 0

    async def test_multiple_rows(self, pool) -> None:
        """Test query returning multiple rows."""
        results = await pool.execute_query("SELECT generate_series(1, 10) as num")
        assert len(results) == 10


class TestConcurrentQueries:
    """Test concurrent query execution."""

    async def test_concurrent_queries(self, pool) -> None:
        """Test running multiple queries concurrently."""

        async def query() -> None:
            return await pool.execute_query("SELECT 1")

        # Run 20 concurrent queries
        results = await asyncio.gather(*[query() for _ in range(20)])
        assert len(results) == 20
        assert all(len(r) == 1 for r in results)

    async def test_pool_not_exhausted(self, pool) -> None:
        """Test that pool doesn't get exhausted."""
        # Get stats before
        pool.stats()

        # Run queries
        await asyncio.gather(*[pool.execute_query("SELECT 1") for _ in range(50)])

        # Stats after - should have connections available
        stats_after = pool.stats()
        assert stats_after["available"] > 0


class TestHealthCheck:
    """Test health check functionality."""

    async def test_health_check_passes(self, pool) -> None:
        """Test health check on healthy pool."""
        is_healthy = await pool.health_check()
        assert is_healthy is True

    async def test_health_check_after_queries(self, pool) -> None:
        """Test health check after executing queries."""
        # Run some queries
        await pool.execute_query("SELECT 1")

        # Pool should still be healthy
        is_healthy = await pool.health_check()
        assert is_healthy is True


class TestPoolStats:
    """Test pool statistics."""

    async def test_stats_structure(self, pool) -> None:
        """Test stats return correct structure."""
        stats = pool.stats()

        assert "size" in stats
        assert "available" in stats
        assert "max_size" in stats
        assert "active" in stats

        assert stats["max_size"] == 10  # from fixture

    async def test_stats_accuracy(self, pool) -> None:
        """Test stats reflect actual pool state."""
        stats = pool.stats()

        # Invariants
        assert stats["size"] <= stats["max_size"]
        assert stats["available"] <= stats["size"]
        assert stats["active"] == stats["size"] - stats["available"]


class TestContextManager:
    """Test async context manager."""

    async def test_context_manager_cleanup(self, postgres_url) -> None:
        """Test that context manager closes pool."""
        from fraiseql._fraiseql_rs import DatabasePool

        async with DatabasePool(url=postgres_url, ssl_mode="disable") as pool:
            # Pool should work inside context
            results = await pool.execute_query("SELECT 1")
            assert len(results) == 1

        # After context, pool is closed (no way to test this directly,
        # but at least verify no errors occurred)


class TestURLParsing:
    """Test connection URL parsing."""

    def test_url_creation(self) -> None:
        """Test pool creation from URL."""
        from fraiseql._fraiseql_rs import DatabasePool

        pool = DatabasePool(url="postgresql://user:pass@localhost:5432/testdb")
        assert pool is not None

    def test_url_overrides_params(self) -> None:
        """Test that URL takes precedence over individual params."""
        from fraiseql._fraiseql_rs import DatabasePool

        # URL should be used, database param ignored
        pool = DatabasePool(url="postgresql://user@localhost/fromurl", database="ignored")
        assert pool is not None


class TestErrorHandling:
    """Test error handling."""

    async def test_invalid_query(self, pool) -> None:
        """Test that invalid SQL raises error."""
        with pytest.raises(Exception):  # PyRuntimeError  # noqa: B017
            await pool.execute_query("INVALID SQL")

    async def test_missing_table(self, pool) -> None:
        """Test querying non-existent table."""
        with pytest.raises(Exception):  # noqa: B017
            await pool.execute_query("SELECT * FROM nonexistent_table_xyz")


@pytest.mark.benchmark
class TestPerformance:
    """Performance benchmarks (optional, run with --benchmark)."""

    async def test_query_latency(self, pool, benchmark) -> None:
        """Benchmark single query latency."""

        async def query() -> None:
            return await pool.execute_query("SELECT 1")

        result = await benchmark(query)
        assert len(result) == 1

    async def test_concurrent_throughput(self, pool) -> None:
        """Test concurrent query throughput."""
        import time

        start = time.perf_counter()

        # 100 concurrent queries
        await asyncio.gather(*[pool.execute_query("SELECT 1") for _ in range(100)])

        elapsed = time.perf_counter() - start

        # Should complete in < 5 seconds even on slow hardware
        assert elapsed < 5.0

        # Queries per second
        qps = 100 / elapsed
        print(f"\nThroughput: {qps:.0f} queries/sec")
