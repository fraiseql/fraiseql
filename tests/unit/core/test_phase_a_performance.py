"""Performance benchmarking for Phase A schema optimization.

Measures the performance impact of using Rust-exported schemas vs Python generation.
"""

import time

import pytest

try:
    from fraiseql import fraiseql_rs
except ImportError:
    fraiseql_rs = None


def skip_if_no_rust() -> None:
    """Skip test if fraiseql_rs is not available."""
    if fraiseql_rs is None:
        pytest.skip("fraiseql_rs not available")


class TestPhaseAPerformance:
    """Performance benchmarks for Phase A schema optimization."""

    def test_schema_loader_cached_access_performance(self, benchmark) -> None:
        """Benchmark cached schema_loader access performance."""
        skip_if_no_rust()
        from fraiseql.gql.schema_loader import load_schema

        # Warm up - ensure schema is loaded and cached
        load_schema()

        # Benchmark cached schema access
        def schema_loader_access():
            schema = load_schema()
            return schema["filter_schemas"]["String"]["fields"]

        result = benchmark(schema_loader_access)
        assert isinstance(result, dict)
        print(f"\nCached schema loader access: {result}")

    def test_rust_schema_export_performance(self, benchmark) -> None:
        """Benchmark Rust schema export FFI call."""
        skip_if_no_rust()

        def export_schema():
            return fraiseql_rs.export_schema_generators()

        result = benchmark(export_schema)
        assert isinstance(result, str)
        print(f"\nRust schema export: {result[:50]}...")

    def test_schema_loader_caching_benefit(self) -> None:
        """Verify caching provides benefit."""
        skip_if_no_rust()
        from fraiseql.gql.schema_loader import load_schema

        # First load
        start = time.perf_counter()
        load_schema()
        first_load_time = time.perf_counter() - start

        # Subsequent loads (cached)
        times = []
        for _ in range(10):
            start = time.perf_counter()
            load_schema()
            times.append(time.perf_counter() - start)

        cached_avg = sum(times) / len(times)
        cache_speedup = first_load_time / cached_avg if cached_avg > 0 else float("inf")

        print(f"\nFirst load: {first_load_time:.6f}s")
        print(f"Cached avg (10 iterations): {cached_avg:.6f}s")
        print(f"Speedup factor: {cache_speedup:.1f}x")

        # Cached should be significantly faster (at least 2x)
        assert cached_avg < first_load_time, "Caching should improve performance"
        assert cache_speedup >= 2.0, f"Caching should provide at least 2x speedup, got {cache_speedup:.1f}x"


class TestPhaseAMemoryUsage:
    """Test memory characteristics of Phase A."""

    def test_schema_loader_memory_efficiency(self) -> None:
        """Verify schema loader uses reasonable memory."""
        skip_if_no_rust()
        import sys

        from fraiseql.gql.schema_loader import load_schema

        schema = load_schema()

        # Get approximate size
        size_bytes = sys.getsizeof(schema)
        size_kb = size_bytes / 1024
        size_mb = size_kb / 1024

        print(f"\nSchema size in memory: {size_bytes} bytes ({size_kb:.1f} KB, {size_mb:.3f} MB)")

        # Schema should be reasonable size (< 1 MB)
        assert size_bytes < 1_000_000, f"Schema too large: {size_mb:.2f} MB"

    def test_multiple_schema_loads_use_same_object(self) -> None:
        """Verify caching prevents duplicate objects."""
        skip_if_no_rust()

        from fraiseql.gql.schema_loader import load_schema

        schemas = [load_schema() for _ in range(5)]

        # All should be the same object
        for schema in schemas[1:]:
            assert schema is schemas[0], "Cached schemas should be identical objects"

        # Memory should not increase
        print(f"\nSchema object ID: {id(schemas[0])}")
        print(f"All 5 loads reference same object: {all(id(s) == id(schemas[0]) for s in schemas)}")


class TestPhaseAIntegrationPerformance:
    """Test performance of integrated schema_loader in generators."""

    def test_where_generator_with_schema_loader(self) -> None:
        """Test WHERE generator can access schema efficiently."""
        skip_if_no_rust()
        from fraiseql.sql.graphql_where_generator import get_filter_schema_from_loader

        # Should be fast - pulling from cache
        start = time.perf_counter()
        schema = get_filter_schema_from_loader("String")
        elapsed = time.perf_counter() - start

        print(f"\nWHERE generator schema access: {elapsed:.6f}s")
        assert schema is not None
        assert "fields" in schema

    def test_order_by_generator_with_schema_loader(self) -> None:
        """Test OrderBy generator can access schema efficiently."""
        skip_if_no_rust()
        from fraiseql.sql.graphql_order_by_generator import get_order_by_schema_from_loader

        # Should be fast - pulling from cache
        start = time.perf_counter()
        schema = get_order_by_schema_from_loader()
        elapsed = time.perf_counter() - start

        print(f"\nOrderBy generator schema access: {elapsed:.6f}s")
        assert schema is not None
        assert "directions" in schema
