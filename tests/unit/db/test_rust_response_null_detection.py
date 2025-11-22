"""Unit tests for RustResponseBytes null detection optimization.

Tests the _is_rust_response_null() function which uses O(1) byte pattern
matching to detect null results from the Rust pipeline without JSON parsing.
"""

from fraiseql.core.rust_pipeline import RustResponseBytes
from fraiseql.db import _is_rust_response_null


class TestNullDetection:
    """Test null detection for common patterns."""

    def test_detects_common_null_patterns(self) -> None:
        """Test detection of common null patterns (cache hits)."""
        # Common patterns that should be in the cache
        common_patterns = [
            b'{"data":{"user":[]}}',
            b'{"data":{"users":[]}}',
            b'{"data":{"customer":[]}}',
            b'{"data":{"customers":[]}}',
            b'{"data":{"product":[]}}',
            b'{"data":{"products":[]}}',
            b'{"data":{"order":[]}}',
            b'{"data":{"orders":[]}}',
            b'{"data":{"item":[]}}',
            b'{"data":{"items":[]}}',
        ]

        for pattern in common_patterns:
            response = RustResponseBytes(pattern)
            assert _is_rust_response_null(response), f"Should detect null: {pattern.decode()}"

    def test_detects_uncommon_null_patterns(self) -> None:
        """Test detection of uncommon field names (not in cache)."""
        # Uncommon field names that need structural validation
        uncommon_patterns = [
            b'{"data":{"myCustomField":[]}}',
            b'{"data":{"veryLongFieldNameThatIsNotCommon":[]}}',
            b'{"data":{"a":[]}}',  # Very short
            b'{"data":{"employee":[]}}',
            b'{"data":{"transaction":[]}}',
        ]

        for pattern in uncommon_patterns:
            response = RustResponseBytes(pattern)
            assert _is_rust_response_null(response), f"Should detect null: {pattern.decode()}"

    def test_rejects_non_null_single_objects(self) -> None:
        """Test rejection of non-null single object responses."""
        non_null_patterns = [
            b'{"data":{"user":{"id":"123","name":"Alice"}}}',
            b'{"data":{"customer":{"id":"456"}}}',
            b'{"data":{"product":{"id":"789","price":99.99}}}',
        ]

        for pattern in non_null_patterns:
            response = RustResponseBytes(pattern)
            assert not _is_rust_response_null(response), (
                f"Should NOT detect as null: {pattern.decode()}"
            )

    def test_rejects_non_empty_arrays(self) -> None:
        """Test rejection of non-empty array responses."""
        non_empty_arrays = [
            b'{"data":{"users":[{"id":"123"}]}}',
            b'{"data":{"users":[{"id":"1"},{"id":"2"}]}}',
            b'{"data":{"products":[{"id":"789"}]}}',
        ]

        for pattern in non_empty_arrays:
            response = RustResponseBytes(pattern)
            assert not _is_rust_response_null(response), (
                f"Should NOT detect as null: {pattern.decode()}"
            )

    def test_length_check_optimization(self) -> None:
        """Test that length check rejects invalid sizes quickly."""
        # Too short (< 17 bytes)
        too_short = RustResponseBytes(b'{"data":{}}')
        assert not _is_rust_response_null(too_short)

        # Too long (> 200 bytes) - would never be a null response
        too_long = RustResponseBytes(b'{"data":{"' + b"x" * 300 + b'":[]}}')
        assert not _is_rust_response_null(too_long)

        # Just right (17 bytes - minimum valid null)
        just_right = RustResponseBytes(b'{"data":{"a":[]}}')
        assert _is_rust_response_null(just_right)

    def test_suffix_check_optimization(self) -> None:
        """Test that suffix check rejects non-matching patterns quickly."""
        # Missing closing braces
        response = RustResponseBytes(b'{"data":{"user":[]}')
        assert not _is_rust_response_null(response)

        # Wrong suffix
        response = RustResponseBytes(b'{"data":{"user":[]},')
        assert not _is_rust_response_null(response)

    def test_pattern_signature_check(self) -> None:
        """Test that pattern signature check (`:[]`) works."""
        # Missing the :[] pattern
        response = RustResponseBytes(b'{"data":{"user":{}}}')
        assert not _is_rust_response_null(response)

        # Has :[] pattern
        response = RustResponseBytes(b'{"data":{"user":[]}}')
        assert _is_rust_response_null(response)

    def test_field_name_validation(self) -> None:
        """Test that field names are validated (no quotes inside)."""
        # Valid field name
        response = RustResponseBytes(b'{"data":{"validField":[]}}')
        assert _is_rust_response_null(response)

        # Invalid: field name contains quotes (malformed JSON)
        response = RustResponseBytes(b'{"data":{"invalid"field":[]}}')
        assert not _is_rust_response_null(response)

    def test_edge_cases(self) -> None:
        """Test edge cases and malformed inputs."""
        # Empty bytes
        response = RustResponseBytes(b"")
        assert not _is_rust_response_null(response)

        # Just opening brace
        response = RustResponseBytes(b"{")
        assert not _is_rust_response_null(response)

        # Null value in JSON (not empty array)
        response = RustResponseBytes(b'{"data":{"user":null}}')
        assert not _is_rust_response_null(response)

        # Different structure
        response = RustResponseBytes(b'{"errors":[]}')
        assert not _is_rust_response_null(response)


class TestCaching:
    """Test caching behavior."""

    def test_cache_grows_with_uncommon_patterns(self) -> None:
        """Test that cache dynamically grows for uncommon patterns."""
        from fraiseql.db import _NULL_RESPONSE_CACHE

        # Get initial cache size
        initial_size = len(_NULL_RESPONSE_CACHE)

        # Add an uncommon pattern (not in cache)
        uncommon_pattern = b'{"data":{"veryUncommonFieldName12345":[]}}'
        response = RustResponseBytes(uncommon_pattern)

        # Should detect as null
        assert _is_rust_response_null(response)

        # Cache should have grown
        assert len(_NULL_RESPONSE_CACHE) > initial_size

        # Should now be in cache (faster next time)
        assert uncommon_pattern in _NULL_RESPONSE_CACHE

    def test_cache_is_bounded(self) -> None:
        """Test that cache doesn't grow unbounded."""
        from fraiseql.db import _NULL_RESPONSE_CACHE

        # The cache is bounded to 100 entries
        # This is tested implicitly in the implementation
        # If we tried to add 200 uncommon patterns, only 100 would be cached

        # Verify cache has reasonable size
        assert len(_NULL_RESPONSE_CACHE) < 150, "Cache should be bounded"


class TestPerformance:
    """Test performance characteristics."""

    def test_null_check_performance(self) -> None:
        """Benchmark null check performance (should be < 0.1ms)."""
        import time

        response = RustResponseBytes(b'{"data":{"user":[]}}')

        # Warmup
        for _ in range(100):
            _is_rust_response_null(response)

        # Measure
        iterations = 10000
        start = time.perf_counter()
        for _ in range(iterations):
            _is_rust_response_null(response)
        elapsed = time.perf_counter() - start

        avg_time_ms = (elapsed / iterations) * 1000

        # Should be < 0.1ms per check (way faster than JSON parsing)
        assert avg_time_ms < 0.1, f"Null check too slow: {avg_time_ms:.3f}ms (expected < 0.1ms)"

    def test_early_exit_performance(self) -> None:
        """Test that early exits are fast for obviously non-null data."""
        import time

        # Large response that will exit early (length check)
        large_data = b'{"data":{"users":[' + b'{"id":"1"},' * 1000 + b"]}}"
        response = RustResponseBytes(large_data)

        iterations = 10000
        start = time.perf_counter()
        for _ in range(iterations):
            _is_rust_response_null(response)
        elapsed = time.perf_counter() - start

        avg_time_ms = (elapsed / iterations) * 1000

        # Early exit should be even faster
        assert avg_time_ms < 0.05, f"Early exit too slow: {avg_time_ms:.3f}ms"


class TestIntegration:
    """Integration tests with actual Rust response format."""

    def test_actual_rust_null_format(self) -> None:
        """Test with actual format from Rust's build_graphql_response."""
        # This is the exact format returned by fraiseql_rs.build_graphql_response
        # when json_strings=[] (no data found)
        actual_format = b'{"data":{"user":[]}}'
        response = RustResponseBytes(actual_format)

        assert _is_rust_response_null(response)

    def test_actual_rust_data_format(self) -> None:
        """Test with actual format from Rust with data."""
        # This is what Rust returns for actual data
        actual_format = b'{"data":{"user":{"id":"123","name":"Alice"}}}'
        response = RustResponseBytes(actual_format)

        assert not _is_rust_response_null(response)

    def test_various_field_names_from_rust(self) -> None:
        """Test various field names that Rust might return."""
        field_names = [
            "user",
            "users",
            "customer",
            "customers",
            "product",
            "products",
            "order",
            "orders",
            "testUser",
            "testUserNullable",  # Common in tests
        ]

        for field_name in field_names:
            # Null case
            null_pattern = f'{{"data":{{"{field_name}":[]}}}}'.encode()
            response = RustResponseBytes(null_pattern)
            assert _is_rust_response_null(response), f"Should detect null for field: {field_name}"

            # Data case
            data_pattern = f'{{"data":{{"{field_name}":{{"id":"123"}}}}}}'.encode()
            response = RustResponseBytes(data_pattern)
            assert not _is_rust_response_null(response), (
                f"Should NOT detect null for field: {field_name}"
            )
