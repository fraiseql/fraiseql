"""Performance benchmarks for DataLoader batch execution.

Tests performance characteristics of the DataLoader including:
- Single entity load latency
- Batch execution throughput
- Cache hit/miss impact
- Deduplication effectiveness
- Large batch handling
"""

import asyncio
import time

import pytest

from fraiseql.federation import clear_entity_registry, entity
from fraiseql.federation.dataloader import EntityDataLoader


class MockAsyncPool:
    """Mock async connection pool for performance testing."""

    def __init__(self, data=None, query_latency_ms=0.1) -> None:
        """Initialize mock pool.

        Args:
            data: Test data dictionary
            query_latency_ms: Simulated database latency in milliseconds
        """
        self.data = data or {}
        self.queries_executed = 0
        self.query_latency_ms = query_latency_ms

    def acquire(self) -> None:
        """Return async context manager for connection."""
        return MockConnectionContext(self)


class MockConnection:
    """Mock database connection."""

    def __init__(self, pool) -> None:
        """Initialize mock connection with reference to pool."""
        self.pool = pool

    async def fetch(self, sql, *params) -> None:  # noqa: ANN002
        """Mock database query execution with configurable latency."""
        # Simulate database latency
        await asyncio.sleep(self.pool.query_latency_ms / 1000.0)

        self.pool.queries_executed += 1

        # Parse the query to extract table and type
        if "tv_user" in sql:
            typename = "User"
            key_field = "id"
        elif "tv_post" in sql:
            typename = "Post"
            key_field = "id"
        else:
            return []

        rows = []
        for key_value in params:
            if (typename, key_value) in self.pool.data:
                entity = self.pool.data[(typename, key_value)]
                rows.append({key_field: key_value, "data": entity})

        return rows


class MockConnectionContext:
    """Async context manager for mock connections."""

    def __init__(self, pool) -> None:
        """Initialize context manager."""
        self.pool = pool
        self.conn = None

    async def __aenter__(self) -> None:
        """Async context manager entry."""
        self.conn = MockConnection(self.pool)
        return self.conn

    async def __aexit__(self, exc_type, exc_val, exc_tb) -> None:
        """Async context manager exit."""


class MockResolver:
    """Mock EntitiesResolver for testing."""


@pytest.fixture
def clear_entities_fixture() -> None:
    """Clear entity registry before and after test."""
    clear_entity_registry()
    yield
    clear_entity_registry()


class TestDataLoaderPerformance:
    """Performance tests for DataLoader batch execution."""

    @pytest.mark.benchmark
    @pytest.mark.asyncio
    async def test_single_entity_latency(self, benchmark, clear_entities_fixture) -> None:
        """Benchmark: Single entity load latency."""

        @entity
        class User:
            id: str

        pool = MockAsyncPool(data={("User", "user-1"): {"name": "Alice"}}, query_latency_ms=1.0)
        loader = EntityDataLoader(MockResolver(), pool, batch_window_ms=1.0)

        async def load_single() -> None:
            return await loader.load("User", "id", "user-1")

        result = await load_single()
        assert result["name"] == "Alice"
        assert pool.queries_executed == 1

    @pytest.mark.benchmark
    @pytest.mark.asyncio
    async def test_batch_throughput_10_entities(self, clear_entities_fixture) -> None:
        """Benchmark: Batch execution of 10 entities."""

        @entity
        class User:
            id: str

        # Create test data
        data = {("User", f"user-{i}"): {"name": f"User{i}"} for i in range(10)}
        pool = MockAsyncPool(data, query_latency_ms=1.0)
        loader = EntityDataLoader(MockResolver(), pool, batch_window_ms=10.0)

        start = time.perf_counter()
        tasks = [asyncio.create_task(loader.load("User", "id", f"user-{i}")) for i in range(10)]
        results = await asyncio.gather(*tasks)
        end = time.perf_counter()

        assert len(results) == 10
        assert all(r is not None for r in results)
        assert pool.queries_executed == 1  # All batched into one query
        latency_ms = (end - start) * 1000
        print(f"\n10 entities batch latency: {latency_ms:.2f}ms")

    @pytest.mark.benchmark
    @pytest.mark.asyncio
    async def test_batch_throughput_100_entities(self, clear_entities_fixture) -> None:
        """Benchmark: Batch execution of 100 entities."""

        @entity
        class User:
            id: str

        # Create test data
        data = {("User", f"user-{i}"): {"name": f"User{i}"} for i in range(100)}
        pool = MockAsyncPool(data, query_latency_ms=2.0)
        loader = EntityDataLoader(MockResolver(), pool, batch_window_ms=10.0)

        start = time.perf_counter()
        tasks = [asyncio.create_task(loader.load("User", "id", f"user-{i}")) for i in range(100)]
        results = await asyncio.gather(*tasks)
        end = time.perf_counter()

        assert len(results) == 100
        assert all(r is not None for r in results)
        assert pool.queries_executed == 1  # All batched into one query
        latency_ms = (end - start) * 1000
        print(f"\n100 entities batch latency: {latency_ms:.2f}ms")

    @pytest.mark.benchmark
    @pytest.mark.asyncio
    async def test_cache_hit_performance(self, clear_entities_fixture) -> None:
        """Benchmark: Cache hit performance (should be instant)."""

        @entity
        class User:
            id: str

        pool = MockAsyncPool(data={("User", "user-1"): {"name": "Alice"}}, query_latency_ms=1.0)
        loader = EntityDataLoader(MockResolver(), pool, batch_window_ms=1.0)

        # First load - cache miss
        result1 = await loader.load("User", "id", "user-1")
        assert result1["name"] == "Alice"
        assert pool.queries_executed == 1

        # Second load - should hit cache
        start = time.perf_counter()
        result2 = await loader.load("User", "id", "user-1")
        end = time.perf_counter()

        assert result2 == result1
        assert pool.queries_executed == 1  # No additional query
        cache_hit_latency_us = (end - start) * 1_000_000
        print(f"\nCache hit latency: {cache_hit_latency_us:.2f}µs")

    @pytest.mark.benchmark
    @pytest.mark.asyncio
    async def test_deduplication_effectiveness(self, clear_entities_fixture) -> None:
        """Benchmark: Deduplication of identical concurrent requests."""

        @entity
        class User:
            id: str

        pool = MockAsyncPool(data={("User", "user-1"): {"name": "Alice"}}, query_latency_ms=1.0)
        loader = EntityDataLoader(MockResolver(), pool, batch_window_ms=10.0)

        # 50 identical concurrent requests
        tasks = [asyncio.create_task(loader.load("User", "id", "user-1")) for _ in range(50)]
        results = await asyncio.gather(*tasks)

        assert len(results) == 50
        assert all(r == results[0] for r in results)
        assert pool.queries_executed == 1  # Only one actual query!

        stats = loader.stats
        print("\nDeduplication stats:")
        print(f"  Total requests: {stats.total_requests}")
        print(f"  Dedup hits: {stats.dedup_hits}")
        print(f"  Dedup rate: {stats.dedup_rate:.1%}")

    @pytest.mark.benchmark
    @pytest.mark.asyncio
    async def test_mixed_hit_miss_performance(self, clear_entities_fixture) -> None:
        """Benchmark: Mixed cache hits and misses."""

        @entity
        class User:
            id: str

        # 20 unique entities
        data = {("User", f"user-{i}"): {"name": f"User{i}"} for i in range(20)}
        pool = MockAsyncPool(data, query_latency_ms=1.0)
        loader = EntityDataLoader(MockResolver(), pool, batch_window_ms=10.0)

        # First pass: load all 20 entities (cache misses)
        tasks1 = [asyncio.create_task(loader.load("User", "id", f"user-{i}")) for i in range(20)]
        results1 = await asyncio.gather(*tasks1)
        queries_after_first = pool.queries_executed

        # Second pass: load same 20 entities again (cache hits)
        start = time.perf_counter()
        tasks2 = [asyncio.create_task(loader.load("User", "id", f"user-{i}")) for i in range(20)]
        results2 = await asyncio.gather(*tasks2)
        end = time.perf_counter()

        assert results1 == results2
        assert pool.queries_executed == queries_after_first  # No new queries!

        cache_pass_latency_ms = (end - start) * 1000
        stats = loader.stats
        print("\nMixed hit/miss stats:")
        print(f"  Cache hit rate: {stats.cache_hit_rate:.1%}")
        print(f"  Second pass (all cache hits): {cache_pass_latency_ms:.2f}ms")

    @pytest.mark.benchmark
    @pytest.mark.asyncio
    async def test_batch_window_impact(self, clear_entities_fixture) -> None:
        """Benchmark: Impact of batch window size on latency."""

        @entity
        class User:
            id: str

        data = {("User", f"user-{i}"): {"name": f"User{i}"} for i in range(10)}

        for window_ms in [0.5, 1.0, 5.0, 10.0]:
            pool = MockAsyncPool(data, query_latency_ms=1.0)
            loader = EntityDataLoader(MockResolver(), pool, batch_window_ms=window_ms)

            start = time.perf_counter()
            tasks = [asyncio.create_task(loader.load("User", "id", f"user-{i}")) for i in range(10)]
            await asyncio.gather(*tasks)
            end = time.perf_counter()

            latency_ms = (end - start) * 1000
            print(f"  Batch window {window_ms}ms: {latency_ms:.2f}ms total latency")

    @pytest.mark.benchmark
    @pytest.mark.asyncio
    async def test_sequential_vs_concurrent(self, clear_entities_fixture) -> None:
        """Benchmark: Sequential vs concurrent request patterns."""

        @entity
        class User:
            id: str

        data = {("User", f"user-{i}"): {"name": f"User{i}"} for i in range(10)}

        # Sequential (await each immediately)
        pool_seq = MockAsyncPool(data, query_latency_ms=1.0)
        loader_seq = EntityDataLoader(MockResolver(), pool_seq, batch_window_ms=1.0)

        start = time.perf_counter()
        for i in range(10):
            await loader_seq.load("User", "id", f"user-{i}")
        seq_time = (time.perf_counter() - start) * 1000

        # Concurrent (create tasks first)
        pool_conc = MockAsyncPool(data, query_latency_ms=1.0)
        loader_conc = EntityDataLoader(MockResolver(), pool_conc, batch_window_ms=10.0)

        start = time.perf_counter()
        tasks = [
            asyncio.create_task(loader_conc.load("User", "id", f"user-{i}")) for i in range(10)
        ]
        await asyncio.gather(*tasks)
        conc_time = (time.perf_counter() - start) * 1000

        print("\nSequential vs Concurrent:")
        print(f"  Sequential: {seq_time:.2f}ms ({pool_seq.queries_executed} queries)")
        print(f"  Concurrent: {conc_time:.2f}ms ({pool_conc.queries_executed} queries)")
        print(f"  Speedup: {seq_time / conc_time:.1f}x")

    @pytest.mark.benchmark
    @pytest.mark.asyncio
    async def test_memory_efficiency_large_batch(self, clear_entities_fixture) -> None:
        """Benchmark: Memory efficiency with large batch."""

        @entity
        class User:
            id: str

        # 1000 entities
        num_entities = 1000
        data = {("User", f"user-{i}"): {"name": f"User{i}"} for i in range(num_entities)}
        pool = MockAsyncPool(data, query_latency_ms=5.0)
        loader = EntityDataLoader(MockResolver(), pool, batch_window_ms=50.0)

        start = time.perf_counter()
        tasks = [
            asyncio.create_task(loader.load("User", "id", f"user-{i}")) for i in range(num_entities)
        ]
        results = await asyncio.gather(*tasks)
        end = time.perf_counter()

        assert len(results) == num_entities
        assert pool.queries_executed == 1

        latency_ms = (end - start) * 1000
        throughput = num_entities / (latency_ms / 1000)

        print("\n1000 entity batch:")
        print(f"  Total latency: {latency_ms:.2f}ms")
        print(f"  Throughput: {throughput:.0f} entities/sec")
