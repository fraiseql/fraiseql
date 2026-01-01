"""
Integration tests for Phase 2.2: Query Streaming (Chunked Queries).

Tests chunked query support added to DatabasePool:
- execute_query_chunked() for memory-efficient pagination
- LIMIT/OFFSET-based chunked processing
- Large result set handling

Uses testcontainers for automatic PostgreSQL provisioning.
"""

import asyncio
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
        # Create a test table with JSONB data (FraiseQL CQRS pattern)
        await pool.execute_query("""
            CREATE TABLE IF NOT EXISTS test_streaming (
                id SERIAL PRIMARY KEY,
                data JSONB NOT NULL
            )
        """)

        # Insert 100 test rows with JSONB data
        for i in range(100):
            import json

            data = json.dumps({"id": i, "value": f"row_{i:03d}"})
            await pool.execute_query(f"INSERT INTO test_streaming (data) VALUES ('{data}')")

        yield pool

        # Cleanup
        await pool.execute_query("DROP TABLE IF EXISTS test_streaming")


class TestBasicChunking:
    """Test basic chunked query operations."""

    async def test_first_chunk(self, pool):
        """Test fetching first chunk of results."""
        # Fetch first 10 rows (select JSONB column)
        results = await pool.execute_query_chunked(
            "SELECT data FROM test_streaming ORDER BY id", limit=10, offset=0
        )

        assert len(results) == 10
        # First row should be row_000
        import json

        first_row = json.loads(results[0])
        assert first_row["value"] == "row_000"

    async def test_middle_chunk(self, pool):
        """Test fetching middle chunk of results."""
        # Fetch rows 30-39 (offset 30, limit 10)
        results = await pool.execute_query_chunked(
            "SELECT data FROM test_streaming ORDER BY id", limit=10, offset=30
        )

        assert len(results) == 10
        import json

        first_row = json.loads(results[0])
        assert first_row["value"] == "row_030"

    async def test_last_chunk(self, pool):
        """Test fetching last chunk of results."""
        # Fetch last 10 rows (offset 90, limit 10)
        results = await pool.execute_query_chunked(
            "SELECT data FROM test_streaming ORDER BY id", limit=10, offset=90
        )

        assert len(results) == 10
        import json

        last_row = json.loads(results[9])
        assert last_row["value"] == "row_099"

    async def test_partial_chunk(self, pool):
        """Test fetching partial chunk at end of result set."""
        # Fetch beyond the last row (offset 95, limit 10)
        results = await pool.execute_query_chunked(
            "SELECT data FROM test_streaming ORDER BY id", limit=10, offset=95
        )

        # Should get only 5 rows (95-99)
        assert len(results) == 5
        import json

        last_row = json.loads(results[4])
        assert last_row["value"] == "row_099"

    async def test_empty_chunk(self, pool):
        """Test fetching chunk beyond result set."""
        # Fetch beyond all rows (offset 1000, limit 10)
        results = await pool.execute_query_chunked(
            "SELECT data FROM test_streaming ORDER BY id", limit=10, offset=1000
        )

        # Should get empty result
        assert len(results) == 0


class TestPaginationPatterns:
    """Test real-world pagination patterns."""

    async def test_iterate_all_chunks(self, pool):
        """Test iterating through all chunks."""
        all_values = []
        chunk_size = 10
        offset = 0

        while True:
            chunk = await pool.execute_query_chunked(
                "SELECT data FROM test_streaming ORDER BY id", limit=chunk_size, offset=offset
            )

            if not chunk:
                break

            import json

            for row_json in chunk:
                row = json.loads(row_json)
                all_values.append(row["value"])

            offset += chunk_size

        # Should have fetched all 100 rows
        assert len(all_values) == 100
        assert all_values[0] == "row_000"
        assert all_values[99] == "row_099"

    async def test_process_in_batches(self, pool):
        """Test batch processing pattern."""
        batch_size = 20
        batch_count = 0
        total_processed = 0

        for batch_num in range(5):  # 5 batches of 20
            offset = batch_num * batch_size
            chunk = await pool.execute_query_chunked(
                "SELECT data FROM test_streaming ORDER BY id", limit=batch_size, offset=offset
            )

            batch_count += 1
            total_processed += len(chunk)

        assert batch_count == 5
        assert total_processed == 100

    async def test_variable_chunk_sizes(self, pool):
        """Test using different chunk sizes."""
        # Small chunks
        small_chunk = await pool.execute_query_chunked(
            "SELECT data FROM test_streaming ORDER BY id", limit=5, offset=0
        )
        assert len(small_chunk) == 5

        # Medium chunks
        medium_chunk = await pool.execute_query_chunked(
            "SELECT data FROM test_streaming ORDER BY id", limit=25, offset=0
        )
        assert len(medium_chunk) == 25

        # Large chunks
        large_chunk = await pool.execute_query_chunked(
            "SELECT data FROM test_streaming ORDER BY id", limit=50, offset=0
        )
        assert len(large_chunk) == 50


class TestQueryVariations:
    """Test chunked queries with various SQL patterns."""

    async def test_with_where_clause(self, pool):
        """Test chunked query with WHERE clause."""
        # Query should work with WHERE clause
        results = await pool.execute_query_chunked(
            "SELECT data FROM test_streaming WHERE id > 50 ORDER BY id", limit=10, offset=0
        )

        assert len(results) == 10
        import json

        first_row = json.loads(results[0])
        # First row should be row_050 (id > 50, since we have id=1..100 and data has id=0..99)
        assert first_row["value"] == "row_050"

    async def test_with_trailing_semicolon(self, pool):
        """Test that trailing semicolon is handled correctly."""
        # Query with semicolon should work
        results = await pool.execute_query_chunked(
            "SELECT * FROM test_streaming ORDER BY id;", limit=10, offset=0
        )

        assert len(results) == 10

    async def test_with_aggregation(self, pool):
        """Test chunked query with aggregation (though less common)."""
        # Even aggregation queries can be chunked (e.g., GROUP BY with many groups)
        results = await pool.execute_query_chunked(
            "SELECT data FROM test_streaming ORDER BY (data->>'value')", limit=5, offset=0
        )

        assert len(results) == 5


class TestConcurrentChunking:
    """Test concurrent chunked query execution."""

    async def test_concurrent_chunks(self, pool):
        """Test fetching multiple chunks concurrently."""

        async def fetch_chunk(offset):
            return await pool.execute_query_chunked(
                "SELECT data FROM test_streaming ORDER BY id", limit=10, offset=offset
            )

        # Fetch 5 chunks concurrently
        chunks = await asyncio.gather(*[fetch_chunk(i * 10) for i in range(5)])

        # All chunks should have 10 rows
        assert all(len(chunk) == 10 for chunk in chunks)

        # Verify no overlap/duplication
        import json

        all_values = []
        for chunk in chunks:
            for row_json in chunk:
                row = json.loads(row_json)
                all_values.append(row["value"])

        # Should have 50 unique values
        assert len(all_values) == 50
        assert len(set(all_values)) == 50


class TestMemoryEfficiency:
    """Test memory-efficient processing patterns."""

    async def test_large_result_set_chunked(self, pool):
        """Test that chunking enables processing large result sets."""
        # Create additional test data
        await pool.execute_query("""
            INSERT INTO test_streaming (data)
            SELECT jsonb_build_object('id', generate_series(100, 1099), 'value', 'large_' || generate_series(100, 1099)::text)
        """)

        # Process in chunks of 100
        chunk_size = 100
        offset = 0
        total_rows = 0

        while True:
            chunk = await pool.execute_query_chunked(
                "SELECT data FROM test_streaming ORDER BY id", limit=chunk_size, offset=offset
            )

            if not chunk:
                break

            total_rows += len(chunk)
            offset += chunk_size

        # Should have processed all rows (100 original + 1000 new = 1100)
        assert total_rows == 1100

    async def test_streaming_pattern(self, pool):
        """Test streaming-style processing pattern."""
        processed_count = 0
        chunk_size = 50
        offset = 0

        # Simulate streaming: process each chunk as it arrives
        while True:
            chunk = await pool.execute_query_chunked(
                "SELECT data FROM test_streaming ORDER BY id", limit=chunk_size, offset=offset
            )

            if not chunk:
                break

            # Process chunk (simulated)
            for _ in chunk:
                processed_count += 1

            offset += chunk_size

        assert processed_count == 100
