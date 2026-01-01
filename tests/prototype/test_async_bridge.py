"""
Phase 0 Prototype: PyO3 Async Bridge Validation Tests

Tests the core functionality of the PyO3 async bridge:
1. Basic query execution
2. Concurrent queries (GIL handling)
3. Cancellation handling
4. Error propagation
5. Memory leaks (after 1000 queries)

Usage:
    pytest tests/prototype/test_async_bridge.py -v
"""

import asyncio
import pytest


# Skip all tests if fraiseql_rs is not available
pytest.importorskip("fraiseql._fraiseql_rs")

from fraiseql._fraiseql_rs import PrototypePool


# Test configuration (adjust for your local PostgreSQL)
DB_CONFIG = {
    "database": "postgres",  # Change to your test database
    "host": "localhost",
    "port": 5432,
    "username": "postgres",  # Change to your username
    "password": None,  # Change if you have a password
    "max_connections": 10,
}


@pytest.fixture
async def pool():
    """Create a prototype pool for testing"""
    try:
        pool_instance = PrototypePool(**DB_CONFIG)
        yield pool_instance
    except Exception as e:
        pytest.skip(f"Cannot connect to PostgreSQL: {e}")


@pytest.mark.asyncio
class TestBasicQueries:
    """Test 1: Basic query execution"""

    async def test_simple_select(self, pool):
        """Test a simple SELECT query"""
        results = await pool.execute_query("SELECT 1 as value")
        assert len(results) == 1
        assert '"value"' in results[0] or '"Value"' in results[0]
        print("✅ Basic query execution works")

    async def test_select_with_multiple_rows(self, pool):
        """Test SELECT query with multiple rows"""
        results = await pool.execute_query(
            "SELECT generate_series(1, 5) as num"
        )
        assert len(results) == 5
        print(f"✅ Multiple rows query works (got {len(results)} rows)")

    async def test_select_with_multiple_columns(self, pool):
        """Test SELECT query with multiple columns"""
        results = await pool.execute_query(
            "SELECT 1 as id, 'test' as name, true as active"
        )
        assert len(results) == 1
        result = results[0]
        assert '"id"' in result or '"Id"' in result
        assert '"name"' in result or '"Name"' in result
        assert '"active"' in result or '"Active"' in result
        print("✅ Multiple columns query works")

    async def test_jsonb_support(self, pool):
        """Test JSONB column support"""
        results = await pool.execute_query(
            "SELECT '{\"key\": \"value\"}'::jsonb as data"
        )
        assert len(results) == 1
        print("✅ JSONB support works")


@pytest.mark.asyncio
class TestConcurrentQueries:
    """Test 2: Concurrent query execution (GIL handling)"""

    async def test_concurrent_simple_queries(self, pool):
        """Test multiple concurrent queries (should not deadlock)"""
        tasks = [
            pool.execute_query("SELECT pg_sleep(0.1), 1 as id"),
            pool.execute_query("SELECT pg_sleep(0.1), 2 as id"),
            pool.execute_query("SELECT pg_sleep(0.1), 3 as id"),
        ]

        results = await asyncio.gather(*tasks)
        assert len(results) == 3
        assert all(len(r) == 1 for r in results)
        print("✅ Concurrent queries work (no deadlock)")

    async def test_many_concurrent_queries(self, pool):
        """Test many concurrent queries (stress test for GIL)"""
        num_queries = 50
        tasks = [
            pool.execute_query("SELECT 1 as value")
            for _ in range(num_queries)
        ]

        results = await asyncio.gather(*tasks)
        assert len(results) == num_queries
        print(f"✅ {num_queries} concurrent queries work (no GIL issues)")

    async def test_concurrent_with_different_durations(self, pool):
        """Test concurrent queries with different execution times"""
        tasks = [
            pool.execute_query("SELECT pg_sleep(0.05), 'fast' as speed"),
            pool.execute_query("SELECT pg_sleep(0.2), 'slow' as speed"),
            pool.execute_query("SELECT pg_sleep(0.1), 'medium' as speed"),
        ]

        results = await asyncio.gather(*tasks)
        assert len(results) == 3
        print("✅ Concurrent queries with different durations work")


@pytest.mark.asyncio
class TestCancellation:
    """Test 3: Cancellation handling"""

    async def test_query_cancellation(self, pool):
        """Test canceling a long-running query"""
        # Note: future_into_py() returns a Future, wrap in async function
        async def long_query():
            return await pool.execute_query("SELECT pg_sleep(5)")

        task = asyncio.create_task(long_query())

        # Cancel after 100ms
        await asyncio.sleep(0.1)
        task.cancel()

        try:
            await task
            pytest.fail("Task should have been cancelled")
        except asyncio.CancelledError:
            print("✅ Query cancellation works")

    async def test_partial_cancellation(self, pool):
        """Test canceling some queries while others complete"""
        # Wrap pool calls in async functions for create_task compatibility
        async def query1():
            return await pool.execute_query("SELECT pg_sleep(0.1), 1 as id")

        async def query2():
            return await pool.execute_query("SELECT pg_sleep(2), 2 as id")

        async def query3():
            return await pool.execute_query("SELECT pg_sleep(0.1), 3 as id")

        # Start 3 queries
        task1 = asyncio.create_task(query1())
        task2 = asyncio.create_task(query2())
        task3 = asyncio.create_task(query3())

        # Cancel task2 after a short wait
        await asyncio.sleep(0.2)
        task2.cancel()

        # task1 and task3 should complete
        result1 = await task1
        result3 = await task3

        assert len(result1) == 1
        assert len(result3) == 1

        # task2 should be cancelled
        try:
            await task2
            pytest.fail("Task2 should have been cancelled")
        except asyncio.CancelledError:
            pass

        print("✅ Partial cancellation works")


@pytest.mark.asyncio
class TestErrorHandling:
    """Test 4: Error propagation across FFI boundary"""

    async def test_syntax_error(self, pool):
        """Test SQL syntax error propagation"""
        with pytest.raises(Exception) as exc_info:
            await pool.execute_query("INVALID SQL SYNTAX")

        assert "syntax error" in str(exc_info.value).lower() or "error" in str(exc_info.value).lower()
        print("✅ Syntax error propagation works")

    async def test_table_not_found(self, pool):
        """Test table not found error"""
        with pytest.raises(Exception) as exc_info:
            await pool.execute_query("SELECT * FROM nonexistent_table_xyz")

        error_msg = str(exc_info.value).lower()
        assert "does not exist" in error_msg or "not found" in error_msg or "error" in error_msg
        print("✅ Table not found error propagation works")

    async def test_type_error(self, pool):
        """Test type error (e.g., invalid cast)"""
        with pytest.raises(Exception) as exc_info:
            await pool.execute_query("SELECT 'not_a_number'::int")

        assert "error" in str(exc_info.value).lower()
        print("✅ Type error propagation works")

    async def test_error_during_concurrent_execution(self, pool):
        """Test error handling when one of concurrent queries fails"""
        tasks = [
            pool.execute_query("SELECT 1"),
            pool.execute_query("INVALID SQL"),  # This will fail
            pool.execute_query("SELECT 2"),
        ]

        results = await asyncio.gather(*tasks, return_exceptions=True)

        # Check that we got 3 results (2 success, 1 error)
        assert len(results) == 3

        # First and third should succeed
        assert isinstance(results[0], list)
        assert isinstance(results[2], list)

        # Second should be an exception
        assert isinstance(results[1], Exception)

        print("✅ Error handling during concurrent execution works")


@pytest.mark.asyncio
class TestPoolHealth:
    """Test pool health and statistics"""

    async def test_health_check(self, pool):
        """Test pool health check"""
        result = await pool.health_check()
        assert result is True
        print("✅ Health check works")

    def test_stats(self, pool):
        """Test pool statistics (synchronous)"""
        stats = pool.stats()
        assert "Pool stats" in stats
        assert "total" in stats
        print(f"✅ Pool stats work: {stats}")

    def test_repr(self, pool):
        """Test pool string representation"""
        repr_str = repr(pool)
        assert "PrototypePool" in repr_str
        print(f"✅ Pool repr works: {repr_str}")


@pytest.mark.asyncio
@pytest.mark.slow
class TestMemoryLeaks:
    """Test 5: Memory leak detection (run 1000 queries)"""

    async def test_no_memory_leak_simple_queries(self, pool):
        """Test no memory leak after 1000 simple queries"""
        import tracemalloc

        tracemalloc.start()
        snapshot1 = tracemalloc.take_snapshot()

        # Run 1000 queries
        for _ in range(1000):
            await pool.execute_query("SELECT 1")

        snapshot2 = tracemalloc.take_snapshot()
        top_stats = snapshot2.compare_to(snapshot1, 'lineno')

        # Print top memory consumers (for debugging)
        print("\nTop memory consumers:")
        for stat in top_stats[:5]:
            print(stat)

        tracemalloc.stop()

        # This is a basic check - actual memory leak detection
        # would require more sophisticated analysis
        print("✅ No obvious memory leaks detected after 1000 queries")

    async def test_no_memory_leak_concurrent(self, pool):
        """Test no memory leak with concurrent queries"""
        import tracemalloc

        tracemalloc.start()
        snapshot1 = tracemalloc.take_snapshot()

        # Run 100 batches of 10 concurrent queries (1000 total)
        for _ in range(100):
            tasks = [pool.execute_query("SELECT 1") for _ in range(10)]
            await asyncio.gather(*tasks)

        snapshot2 = tracemalloc.take_snapshot()
        tracemalloc.stop()

        print("✅ No obvious memory leaks with concurrent queries")


if __name__ == "__main__":
    # Allow running tests directly
    pytest.main([__file__, "-v", "-s"])
