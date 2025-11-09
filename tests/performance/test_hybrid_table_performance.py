"""Performance benchmarks for hybrid table SQL generation."""

import time

from fraiseql.db import FraiseQLRepository, _table_metadata, register_type_for_view


class MockPool:
    """Mock connection pool for performance testing."""

    def connection(self) -> None:
        return MockConnection()


class MockConnection:
    """Mock connection for performance testing."""

    async def __aenter__(self) -> None:
        return self

    async def __aexit__(self, *args) -> None:
        pass

    def cursor(self) -> None:
        return MockCursor()


class MockCursor:
    """Mock cursor that simulates information_schema queries."""

    async def __aenter__(self) -> None:
        return self

    async def __aexit__(self, *args) -> None:
        pass

    async def execute(self, query, params=None) -> None:
        # Simulate slow information_schema query
        if "information_schema" in query:
            time.sleep(0.01)  # Simulate 10ms DB query

    async def fetchall(self) -> None:
        # Return mock column data
        return [
            {"column_name": "id", "data_type": "uuid", "udt_name": "uuid"},
            {"column_name": "is_current", "data_type": "boolean", "udt_name": "bool"},
            {"column_name": "data", "data_type": "jsonb", "udt_name": "jsonb"},
        ]


class TestHybridPerformance:
    """Benchmark SQL generation performance for hybrid tables."""

    def test_where_clause_generation_with_metadata(self) -> None:
        """Test WHERE clause generation speed with pre-registered metadata."""
        # Register with metadata (happens at import time normally)
        register_type_for_view(
            "products",
            type,
            table_columns={"id", "status", "is_active", "created_at", "data"},
            has_jsonb_data=True,
        )

        pool = MockPool()
        repo = FraiseQLRepository(pool, context={})

        # Warm up caches
        repo._should_use_jsonb_path_sync("products", "status")

        # Measure WHERE clause generation time
        start = time.perf_counter()
        for _ in range(1000):
            # This should use cached metadata, no DB queries
            use_jsonb = repo._should_use_jsonb_path_sync("products", "status")
            assert use_jsonb is False  # Regular column

            use_jsonb = repo._should_use_jsonb_path_sync("products", "brand")
            assert use_jsonb is True  # JSONB field

        end = time.perf_counter()
        elapsed_ms = (end - start) * 1000

        # Should be very fast with metadata
        assert elapsed_ms < 10, f"WHERE clause generation took {elapsed_ms:.2f}ms for 2000 checks"
        print(f"\n✅ Performance with metadata: {elapsed_ms:.2f}ms for 2000 field checks")
        print(f"   Average: {elapsed_ms / 2000:.4f}ms per field check")

    def test_where_clause_generation_without_metadata(self) -> None:
        """Test WHERE clause generation speed without pre-registered metadata."""
        # Clear any existing metadata
        if "unknown_table" in _table_metadata:
            del _table_metadata["unknown_table"]

        pool = MockPool()
        repo = FraiseQLRepository(pool, context={})

        # Clear caches to simulate cold start
        if hasattr(repo, "_field_path_cache"):
            repo._field_path_cache.clear()

        # Measure WHERE clause generation time without metadata
        start = time.perf_counter()
        for _ in range(1000):
            # This has to use heuristics since no metadata
            use_jsonb = repo._should_use_jsonb_path_sync("unknown_table", "status")
            assert use_jsonb is False  # Heuristic: known column pattern

            use_jsonb = repo._should_use_jsonb_path_sync("unknown_table", "brand")
            assert use_jsonb is False  # Conservative: assume regular column

        end = time.perf_counter()
        elapsed_ms = (end - start) * 1000

        # Still fast with heuristics, but less accurate
        assert elapsed_ms < 20, f"WHERE clause generation took {elapsed_ms:.2f}ms for 2000 checks"
        print(f"\n⚠️  Performance without metadata: {elapsed_ms:.2f}ms for 2000 field checks")
        print(f"   Average: {elapsed_ms / 2000:.4f}ms per field check")
        print("   Note: Falls back to heuristics which may be less accurate")

    def test_metadata_memory_overhead(self) -> None:
        """Test memory overhead of storing metadata at registration time."""
        import sys

        # Measure size of metadata for a typical hybrid table
        metadata = {
            "columns": {
                "id",
                "tenant_id",
                "name",
                "status",
                "is_active",
                "is_featured",
                "category_id",
                "created_date",
                "data",
            },
            "has_jsonb_data": True,
        }

        size_bytes = sys.getsizeof(metadata) + sum(sys.getsizeof(v) for v in metadata.values())
        size_kb = size_bytes / 1024

        print(f"\n📊 Memory overhead per table: {size_bytes} bytes ({size_kb:.2f} KB)")
        print(f"   For 100 tables: {size_bytes * 100 / 1024:.2f} KB")
        print(f"   For 1000 tables: {size_bytes * 1000 / 1024:.2f} KB")

        # Very minimal memory overhead
        assert size_bytes < 1000, f"Metadata too large: {size_bytes} bytes"
