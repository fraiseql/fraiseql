"""Tests for PostgreSQL cursor-based pagination.

🚀 Uses FraiseQL's UNIFIED CONTAINER system - see database_conftest.py
A single PostgreSQL container runs for ALL tests with socket communication.
"""

import base64

import psycopg
import pytest
import pytest_asyncio

# Import database fixtures for this database test
from tests.fixtures.database.database_conftest import *  # noqa: F403

from fraiseql.cqrs.pagination import CursorPaginator, PaginationParams, decode_cursor, encode_cursor
from fraiseql.cqrs.repository import CQRSRepository

pytestmark = [pytest.mark.integration, pytest.mark.database]


class TestCursorEncoding:
    """Test cursor encoding and decoding functions."""

    def test_encode_cursor(self) -> None:
        """Test cursor encoding to base64."""
        value = "2024-01-15T10:30:00Z"
        encoded = encode_cursor(value)

        # Should be valid base64
        assert isinstance(encoded, str)
        assert len(encoded) > 0

        # Should be decodable
        decoded = base64.b64decode(encoded).decode()
        assert decoded == value

    def test_decode_cursor(self) -> None:
        """Test cursor decoding from base64."""
        value = "user-123"
        encoded = base64.b64encode(value.encode()).decode()
        decoded = decode_cursor(encoded)

        assert decoded == value

    def test_encode_decode_roundtrip(self) -> None:
        """Test encoding and decoding roundtrip."""
        values = [
            """simple"""
            """with spaces"""
            """2024-01-15T10:30:00Z"""
            """123456"""
            """unicode-émoji-🚀"""
        ]

        for value in values:
            encoded = encode_cursor(value)
            decoded = decode_cursor(encoded)
            assert decoded == value


class TestPaginationParams:
    """Test PaginationParams validation and defaults."""

    def test_default_params(self) -> None:
        """Test default pagination parameters."""
        params = PaginationParams()

        assert params.first == 20  # Default page size
        assert params.after is None
        assert params.last is None
        assert params.before is None
        assert params.order_by == "id"
        assert params.order_direction == "ASC"
        assert params.is_forward is True
        assert params.is_backward is False

    def test_forward_pagination(self) -> None:
        """Test forward pagination parameters."""
        params = PaginationParams(first=10, after="cursor123")

        assert params.first == 10
        assert params.after == "cursor123"
        assert params.is_forward is True
        assert params.is_backward is False

    def test_backward_pagination(self) -> None:
        """Test backward pagination parameters."""
        params = PaginationParams(last=5, before="cursor456")

        assert params.last == 5
        assert params.before == "cursor456"
        assert params.is_forward is False
        assert params.is_backward is True

    def test_order_direction_normalization(self) -> None:
        """Test order direction is normalized to uppercase."""
        params1 = PaginationParams(order_direction="asc")
        assert params1.order_direction == "ASC"

        params2 = PaginationParams(order_direction="desc")
        assert params2.order_direction == "DESC"

    def test_invalid_first_and_last(self) -> None:
        """Test that specifying both first and last raises error."""
        with pytest.raises(ValueError, match="Cannot specify both"):
            PaginationParams(first=10, last=10)

    def test_negative_first(self) -> None:
        """Test that negative first raises error."""
        with pytest.raises(ValueError, match="must be non-negative"):
            PaginationParams(first=-1)

    def test_negative_last(self) -> None:
        """Test that negative last raises error."""
        with pytest.raises(ValueError, match="must be non-negative"):
            PaginationParams(last=-1)


@pytest_asyncio.fixture(scope="class")
async def setup_pagination_tables(class_db_pool, test_schema):
    """Set up test tables for pagination tests using unified container system."""
    async with class_db_pool.connection() as conn:
        await conn.execute(f"SET search_path TO {test_schema}, public")

        # Create test table
        await conn.execute(
            """
            CREATE TABLE IF NOT EXISTS items (
                id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
                data JSONB NOT NULL
            )
        """
        )

        # Create view
        await conn.execute(
            """
            CREATE OR REPLACE VIEW v_items AS
            SELECT id, data FROM items
        """
        )
        await conn.commit()

    yield

    # No cleanup needed - schema will be dropped automatically


@pytest.mark.database
class TestCursorPaginator:
    """Test CursorPaginator functionality with real database."""

    async def setup_test_data(self, conn) -> None:
        """Insert test data into the database."""
        async with conn.cursor() as cursor:
            # Insert test items
            test_data = [
                {"id": "id1", "name": "Item 1", "createdAt": "2024-01-01"},
                {"id": "id2", "name": "Item 2", "createdAt": "2024-01-02"},
                {"id": "id3", "name": "Item 3", "createdAt": "2024-01-03"},
                {"id": "id4", "name": "Item 4", "createdAt": "2024-01-04"},
                {"id": "id5", "name": "Item 5", "createdAt": "2024-01-05"},
            ]

            for item in test_data:
                # Generate proper UUID format
                uuid_num = int(item["id"].replace("id", ""))
                uuid_str = f"00000000-0000-0000-0000-{uuid_num:012d}"
                await cursor.execute(
                    "INSERT INTO items (id, data) VALUES (%s::uuid, %s::jsonb)",
                    (uuid_str, psycopg.types.json.Json(item)),
                )

            # No commit needed - within test transaction

    @pytest.mark.asyncio
    async def test_paginate_forward_basic(
        self, class_db_pool, test_schema, setup_pagination_tables
    ) -> None:
        """Test basic forward pagination."""
        async with class_db_pool.connection() as conn:
            await conn.execute(f"SET search_path TO {test_schema}, public")
            await conn.execute("TRUNCATE TABLE items CASCADE")
            await self.setup_test_data(conn)

            paginator = CursorPaginator(conn)
            params = PaginationParams(first=2, order_by="createdAt")
            result = await paginator.paginate("v_items", params)

            # Check structure
            assert "edges" in result
            assert "page_info" in result
            assert "total_count" in result

            # Check edges
            assert len(result["edges"]) == 2
            assert result["edges"][0]["node"]["name"] == "Item 1"
            assert result["edges"][1]["node"]["name"] == "Item 2"

            # Check cursors are encoded
            assert result["edges"][0]["cursor"] == encode_cursor("2024-01-01")
            assert result["edges"][1]["cursor"] == encode_cursor("2024-01-02")

            # Check page info
            assert result["page_info"]["has_next_page"] is True
            assert result["page_info"]["has_previous_page"] is False
            assert result["page_info"]["start_cursor"] == encode_cursor("2024-01-01")
            assert result["page_info"]["end_cursor"] == encode_cursor("2024-01-02")
            assert result["total_count"] == 5

    @pytest.mark.asyncio
    async def test_paginate_with_after_cursor(
        self, class_db_pool, test_schema, setup_pagination_tables
    ) -> None:
        """Test pagination with after cursor."""
        async with class_db_pool.connection() as conn:
            await conn.execute(f"SET search_path TO {test_schema}, public")
            await conn.execute("TRUNCATE TABLE items CASCADE")
            await self.setup_test_data(conn)

            paginator = CursorPaginator(conn)
            params = PaginationParams(
                first=2, after=encode_cursor("2024-01-02"), order_by="createdAt"
            )
            result = await paginator.paginate("v_items", params, include_total=False)

            # Should return items 3 and 4
            assert len(result["edges"]) == 2
            assert result["edges"][0]["node"]["name"] == "Item 3"
            assert result["edges"][1]["node"]["name"] == "Item 4"
            assert result["page_info"]["has_next_page"] is True

    @pytest.mark.asyncio
    async def test_paginate_backward(
        self, class_db_pool, test_schema, setup_pagination_tables
    ) -> None:
        """Test backward pagination."""
        async with class_db_pool.connection() as conn:
            await conn.execute(f"SET search_path TO {test_schema}, public")
            await conn.execute("TRUNCATE TABLE items CASCADE")
            await self.setup_test_data(conn)

            paginator = CursorPaginator(conn)
            params = PaginationParams(
                last=2, before=encode_cursor("2024-01-04"), order_by="createdAt"
            )
            result = await paginator.paginate("v_items", params, include_total=False)

            # Should return items 2 and 3 (in forward order after reversal)
            assert len(result["edges"]) == 2
            assert result["edges"][0]["node"]["name"] == "Item 2"
            assert result["edges"][1]["node"]["name"] == "Item 3"
            assert result["page_info"]["has_previous_page"] is True

    @pytest.mark.asyncio
    async def test_paginate_with_filters(
        self, class_db_pool, test_schema, setup_pagination_tables
    ) -> None:
        """Test pagination with filters."""
        async with class_db_pool.connection() as conn:
            await conn.execute(f"SET search_path TO {test_schema}, public")
            await conn.execute("TRUNCATE TABLE items CASCADE")
            await self.setup_test_data(conn)

            # Add some items with different attributes
            async with conn.cursor() as cursor:
                await cursor.execute(
                    "INSERT INTO items (data) VALUES (%s::jsonb)",
                    (
                        psycopg.types.json.Json(
                            {"name": "Special Item", "createdAt": "2024-01-06", "special": True}
                        ),
                    ),
                )
                # No commit needed - within test transaction

            paginator = CursorPaginator(conn)
            params = PaginationParams(first=10, order_by="createdAt")

            # Filter for special items
            result = await paginator.paginate("v_items", params, filters={"special": "true"})

            assert len(result["edges"]) == 1
            assert result["edges"][0]["node"]["name"] == "Special Item"
            assert result["total_count"] == 1

    @pytest.mark.asyncio
    async def test_empty_results(self, class_db_pool, test_schema, setup_pagination_tables) -> None:
        """Test pagination with empty results."""
        async with class_db_pool.connection() as conn:
            await conn.execute(f"SET search_path TO {test_schema}, public")
            await conn.execute("TRUNCATE TABLE items CASCADE")
            # Don't insert any data
            paginator = CursorPaginator(conn)
            params = PaginationParams(first=10)
            result = await paginator.paginate("v_items", params)

            assert len(result["edges"]) == 0
            assert result["page_info"]["has_next_page"] is False
            assert result["page_info"]["has_previous_page"] is False
            assert result["page_info"]["start_cursor"] is None
            assert result["page_info"]["end_cursor"] is None
            assert result["total_count"] == 0


@pytest.mark.database
class TestRepositoryIntegration:
    """Test pagination integration with CQRSRepository."""

    @pytest.mark.asyncio
    async def test_repository_paginate_method(
        self, class_db_pool, test_schema, setup_pagination_tables
    ) -> None:
        """Test the paginate method on CQRSRepository."""
        async with class_db_pool.connection() as conn:
            await conn.execute(f"SET search_path TO {test_schema}, public")
            await conn.execute("TRUNCATE TABLE items CASCADE")

            # Set up test data
            test_items = [
                {"id": f"id{i}", "name": f"Item {i}", "createdAt": f"2024-01-{i:02d}"}
                for i in range(1, 11)
            ]

            for item in test_items:
                # Generate proper UUID format
                uuid_num = int(item["id"].replace("id", ""))
                uuid_str = f"00000000-0000-0000-0000-{uuid_num:012d}"
                await conn.execute(
                    "INSERT INTO items (id, data) VALUES (%s::uuid, %s::jsonb)",
                    (uuid_str, psycopg.types.json.Json(item)),
                )

            # Test pagination through repository
            repo = CQRSRepository(conn)
            result = await repo.paginate(
                """v_items""", first=5, order_by="createdAt", order_direction="DESC"
            )

            # Should return items in descending order
            assert len(result["edges"]) == 5
            assert result["edges"][0]["node"]["name"] == "Item 10"
            assert result["edges"][-1]["node"]["name"] == "Item 6"
            assert result["page_info"]["has_next_page"] is True
            assert result["total_count"] == 10
