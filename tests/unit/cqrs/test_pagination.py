"""Tests for CQRS pagination module."""

from unittest.mock import AsyncMock, MagicMock
from uuid import uuid4

import pytest

from fraiseql.cqrs.pagination import (
    CursorPaginator,
    PaginationParams,
    decode_cursor,
    encode_cursor,
    paginate_query,
)


# Mock cursor context manager
class MockCursorContext:
    """Mock async cursor context manager."""

    def __init__(self, cursor: AsyncMock) -> None:
        self.cursor = cursor

    async def __aenter__(self) -> AsyncMock:
        return self.cursor

    async def __aexit__(self, *args: object) -> None:
        pass


@pytest.fixture
def mock_cursor() -> AsyncMock:
    """Create a mock cursor."""
    cursor = AsyncMock()
    cursor.execute = AsyncMock()
    cursor.fetchone = AsyncMock(return_value=None)
    cursor.fetchall = AsyncMock(return_value=[])
    return cursor


@pytest.fixture
def mock_connection(mock_cursor: AsyncMock) -> MagicMock:
    """Create a mock connection."""
    connection = MagicMock()
    connection.cursor = MagicMock(return_value=MockCursorContext(mock_cursor))
    return connection


# Tests for cursor encoding/decoding
@pytest.mark.unit
class TestCursorEncoding:
    """Tests for cursor encoding and decoding."""

    def test_encode_cursor_base64(self) -> None:
        """Encode cursor to base64."""
        result = encode_cursor("test-value-123")
        # Base64 of "test-value-123"
        assert result == "dGVzdC12YWx1ZS0xMjM="

    def test_decode_cursor_base64(self) -> None:
        """Decode base64 cursor."""
        result = decode_cursor("dGVzdC12YWx1ZS0xMjM=")
        assert result == "test-value-123"

    def test_cursor_roundtrip(self) -> None:
        """Encode and decode produces original value."""
        original = "my-cursor-value-abc123"
        encoded = encode_cursor(original)
        decoded = decode_cursor(encoded)
        assert decoded == original

    @pytest.mark.parametrize(
        "value",
        [
            "simple",
            "with spaces",
            "with/slashes",
            "with+plus",
            "uuid-like-12345678-1234-1234-1234-123456789abc",
            "123",
            "",
        ],
    )
    def test_cursor_roundtrip_various_values(self, value: str) -> None:
        """Cursor roundtrip works for various values."""
        assert decode_cursor(encode_cursor(value)) == value


# Tests for PaginationParams
@pytest.mark.unit
class TestPaginationParams:
    """Tests for PaginationParams class."""

    def test_first_only(self) -> None:
        """PaginationParams with first only."""
        params = PaginationParams(first=10)
        assert params.first == 10
        assert params.last is None
        assert params.is_forward is True
        assert params.is_backward is False

    def test_last_only(self) -> None:
        """PaginationParams with last only."""
        params = PaginationParams(last=10)
        assert params.first is None
        assert params.last == 10
        assert params.is_forward is False
        assert params.is_backward is True

    def test_first_and_last_raises_error(self) -> None:
        """Cannot specify both first and last."""
        with pytest.raises(ValueError, match="Cannot specify both"):
            PaginationParams(first=10, last=10)

    def test_negative_first_raises_error(self) -> None:
        """Negative first raises error."""
        with pytest.raises(ValueError, match="'first' must be non-negative"):
            PaginationParams(first=-1)

    def test_negative_last_raises_error(self) -> None:
        """Negative last raises error."""
        with pytest.raises(ValueError, match="'last' must be non-negative"):
            PaginationParams(last=-1)

    def test_default_first_when_none_specified(self) -> None:
        """Default first=20 when neither first nor last specified."""
        params = PaginationParams()
        assert params.first == 20
        assert params.last is None

    def test_order_direction_normalized_to_upper(self) -> None:
        """Order direction is normalized to uppercase."""
        params_lower = PaginationParams(first=10, order_direction="asc")
        params_upper = PaginationParams(first=10, order_direction="DESC")

        assert params_lower.order_direction == "ASC"
        assert params_upper.order_direction == "DESC"

    def test_after_cursor(self) -> None:
        """PaginationParams stores after cursor."""
        params = PaginationParams(first=10, after="cursor123")
        assert params.after == "cursor123"
        assert params.before is None

    def test_before_cursor(self) -> None:
        """PaginationParams stores before cursor."""
        params = PaginationParams(last=10, before="cursor456")
        assert params.before == "cursor456"
        assert params.after is None

    def test_order_by_default(self) -> None:
        """Order by defaults to 'id'."""
        params = PaginationParams(first=10)
        assert params.order_by == "id"

    def test_order_by_custom(self) -> None:
        """Custom order_by is stored."""
        params = PaginationParams(first=10, order_by="created_at")
        assert params.order_by == "created_at"

    def test_zero_first_is_valid(self) -> None:
        """Zero is a valid value for first."""
        params = PaginationParams(first=0)
        assert params.first == 0

    def test_zero_last_is_valid(self) -> None:
        """Zero is a valid value for last."""
        params = PaginationParams(last=0)
        assert params.last == 0


# Tests for CursorPaginator
@pytest.mark.unit
class TestCursorPaginator:
    """Tests for CursorPaginator class."""

    @pytest.mark.asyncio
    async def test_paginate_forward(
        self, mock_connection: MagicMock, mock_cursor: AsyncMock
    ) -> None:
        """Paginate forward returns edges and page_info."""
        id1, id2 = uuid4(), uuid4()
        mock_cursor.fetchall.return_value = [
            (id1, {"id": str(id1), "name": "A"}),
            (id2, {"id": str(id2), "name": "B"}),
        ]
        mock_cursor.fetchone.return_value = (2,)  # total count

        paginator = CursorPaginator(mock_connection)
        params = PaginationParams(first=10)
        result = await paginator.paginate("v_users", params)

        assert "edges" in result
        assert "page_info" in result
        assert len(result["edges"]) == 2
        assert result["edges"][0]["node"]["name"] == "A"
        assert result["edges"][1]["node"]["name"] == "B"

    @pytest.mark.asyncio
    async def test_paginate_backward(
        self, mock_connection: MagicMock, mock_cursor: AsyncMock
    ) -> None:
        """Paginate backward reverses results."""
        id1, id2 = uuid4(), uuid4()
        # Backward pagination fetches in reverse, then we reverse again
        mock_cursor.fetchall.return_value = [
            (id2, {"id": str(id2), "name": "B"}),
            (id1, {"id": str(id1), "name": "A"}),
        ]
        mock_cursor.fetchone.return_value = (2,)

        paginator = CursorPaginator(mock_connection)
        params = PaginationParams(last=10)
        result = await paginator.paginate("v_users", params)

        # Results should be reversed for backward pagination
        assert len(result["edges"]) == 2
        # After reversal: A comes first, B comes second
        assert result["edges"][0]["node"]["name"] == "A"
        assert result["edges"][1]["node"]["name"] == "B"

    @pytest.mark.asyncio
    async def test_paginate_with_filters(
        self, mock_connection: MagicMock, mock_cursor: AsyncMock
    ) -> None:
        """Paginate applies filters."""
        mock_cursor.fetchall.return_value = []
        mock_cursor.fetchone.return_value = (0,)

        paginator = CursorPaginator(mock_connection)
        params = PaginationParams(first=10)
        result = await paginator.paginate("v_users", params, filters={"status": "active"})

        assert result["edges"] == []
        mock_cursor.execute.assert_called()

    @pytest.mark.asyncio
    async def test_paginate_with_array_filter(
        self, mock_connection: MagicMock, mock_cursor: AsyncMock
    ) -> None:
        """Paginate handles array containment filters."""
        mock_cursor.fetchall.return_value = []
        mock_cursor.fetchone.return_value = (0,)

        paginator = CursorPaginator(mock_connection)
        params = PaginationParams(first=10)
        await paginator.paginate("v_users", params, filters={"roles": ["admin", "user"]})

        mock_cursor.execute.assert_called()

    @pytest.mark.asyncio
    async def test_paginate_with_after_cursor(
        self, mock_connection: MagicMock, mock_cursor: AsyncMock
    ) -> None:
        """Paginate uses after cursor for positioning."""
        mock_cursor.fetchall.return_value = []
        mock_cursor.fetchone.return_value = (0,)

        cursor = encode_cursor("cursor-value")
        paginator = CursorPaginator(mock_connection)
        params = PaginationParams(first=10, after=cursor)
        await paginator.paginate("v_users", params)

        mock_cursor.execute.assert_called()

    @pytest.mark.asyncio
    async def test_paginate_with_before_cursor(
        self, mock_connection: MagicMock, mock_cursor: AsyncMock
    ) -> None:
        """Paginate uses before cursor for positioning."""
        mock_cursor.fetchall.return_value = []
        mock_cursor.fetchone.return_value = (0,)

        cursor = encode_cursor("cursor-value")
        paginator = CursorPaginator(mock_connection)
        params = PaginationParams(last=10, before=cursor)
        await paginator.paginate("v_users", params)

        mock_cursor.execute.assert_called()

    @pytest.mark.asyncio
    async def test_has_next_page_true(
        self, mock_connection: MagicMock, mock_cursor: AsyncMock
    ) -> None:
        """has_next_page is True when there are more items."""
        # Return one extra item to indicate more pages
        mock_cursor.fetchall.return_value = [
            (uuid4(), {"id": "1", "name": "A"}),
            (uuid4(), {"id": "2", "name": "B"}),
            (uuid4(), {"id": "3", "name": "C"}),  # Extra item
        ]
        mock_cursor.fetchone.return_value = (10,)

        paginator = CursorPaginator(mock_connection)
        params = PaginationParams(first=2)
        result = await paginator.paginate("v_users", params)

        assert result["page_info"]["has_next_page"] is True
        assert len(result["edges"]) == 2  # Extra item removed

    @pytest.mark.asyncio
    async def test_has_next_page_false(
        self, mock_connection: MagicMock, mock_cursor: AsyncMock
    ) -> None:
        """has_next_page is False when no more items."""
        mock_cursor.fetchall.return_value = [
            (uuid4(), {"id": "1", "name": "A"}),
        ]
        mock_cursor.fetchone.return_value = (1,)

        paginator = CursorPaginator(mock_connection)
        params = PaginationParams(first=10)
        result = await paginator.paginate("v_users", params)

        assert result["page_info"]["has_next_page"] is False

    @pytest.mark.asyncio
    async def test_has_previous_page_true_backward(
        self, mock_connection: MagicMock, mock_cursor: AsyncMock
    ) -> None:
        """has_previous_page is True for backward pagination with extra items."""
        # Return one extra item to indicate more pages
        mock_cursor.fetchall.return_value = [
            (uuid4(), {"id": "3", "name": "C"}),
            (uuid4(), {"id": "2", "name": "B"}),
            (uuid4(), {"id": "1", "name": "A"}),  # Extra item
        ]
        mock_cursor.fetchone.return_value = (10,)

        cursor = encode_cursor("cursor-5")
        paginator = CursorPaginator(mock_connection)
        params = PaginationParams(last=2, before=cursor)
        result = await paginator.paginate("v_users", params)

        # Backward pagination: has_previous_page = has_extra
        # We returned 3 items for limit=2, so has_extra = True
        assert result["page_info"]["has_previous_page"] is True

    @pytest.mark.asyncio
    async def test_has_previous_page_false(
        self, mock_connection: MagicMock, mock_cursor: AsyncMock
    ) -> None:
        """has_previous_page is False at beginning of list."""
        mock_cursor.fetchall.return_value = [
            (uuid4(), {"id": "1", "name": "A"}),
        ]
        mock_cursor.fetchone.return_value = (1,)

        paginator = CursorPaginator(mock_connection)
        params = PaginationParams(first=10)  # No after cursor
        result = await paginator.paginate("v_users", params)

        # No after cursor means we're at the beginning
        # has_previous_page = (after is not None) = False
        assert result["page_info"]["has_previous_page"] is False

    @pytest.mark.asyncio
    async def test_empty_results(self, mock_connection: MagicMock, mock_cursor: AsyncMock) -> None:
        """Empty results return empty edges and null cursors."""
        mock_cursor.fetchall.return_value = []
        mock_cursor.fetchone.return_value = (0,)

        paginator = CursorPaginator(mock_connection)
        params = PaginationParams(first=10)
        result = await paginator.paginate("v_users", params)

        assert result["edges"] == []
        assert result["page_info"]["start_cursor"] is None
        assert result["page_info"]["end_cursor"] is None
        assert result["page_info"]["has_next_page"] is False
        assert result["page_info"]["has_previous_page"] is False

    @pytest.mark.asyncio
    async def test_total_count_included(
        self, mock_connection: MagicMock, mock_cursor: AsyncMock
    ) -> None:
        """Total count is included when requested."""
        mock_cursor.fetchall.return_value = [
            (uuid4(), {"id": "1", "name": "A"}),
        ]
        mock_cursor.fetchone.return_value = (42,)

        paginator = CursorPaginator(mock_connection)
        params = PaginationParams(first=10)
        result = await paginator.paginate("v_users", params, include_total=True)

        assert result["total_count"] == 42
        assert result["page_info"]["total_count"] == 42

    @pytest.mark.asyncio
    async def test_total_count_not_included(
        self, mock_connection: MagicMock, mock_cursor: AsyncMock
    ) -> None:
        """Total count is not included when not requested."""
        mock_cursor.fetchall.return_value = [
            (uuid4(), {"id": "1", "name": "A"}),
        ]

        paginator = CursorPaginator(mock_connection)
        params = PaginationParams(first=10)
        result = await paginator.paginate("v_users", params, include_total=False)

        assert result["total_count"] is None

    @pytest.mark.asyncio
    async def test_cursor_values_in_edges(
        self, mock_connection: MagicMock, mock_cursor: AsyncMock
    ) -> None:
        """Each edge has a cursor value."""
        id1 = uuid4()
        mock_cursor.fetchall.return_value = [
            (id1, {"id": str(id1), "name": "A"}),
        ]
        mock_cursor.fetchone.return_value = (1,)

        paginator = CursorPaginator(mock_connection)
        params = PaginationParams(first=10)
        result = await paginator.paginate("v_users", params)

        assert "cursor" in result["edges"][0]
        # Cursor should be base64 encoded
        decoded = decode_cursor(result["edges"][0]["cursor"])
        assert decoded == str(id1)

    @pytest.mark.asyncio
    async def test_start_and_end_cursors(
        self, mock_connection: MagicMock, mock_cursor: AsyncMock
    ) -> None:
        """page_info has start_cursor and end_cursor."""
        id1, id2 = uuid4(), uuid4()
        mock_cursor.fetchall.return_value = [
            (id1, {"id": str(id1), "name": "A"}),
            (id2, {"id": str(id2), "name": "B"}),
        ]
        mock_cursor.fetchone.return_value = (2,)

        paginator = CursorPaginator(mock_connection)
        params = PaginationParams(first=10)
        result = await paginator.paginate("v_users", params)

        assert result["page_info"]["start_cursor"] == result["edges"][0]["cursor"]
        assert result["page_info"]["end_cursor"] == result["edges"][-1]["cursor"]


# Tests for _get_total_count
@pytest.mark.unit
class TestGetTotalCount:
    """Tests for _get_total_count method."""

    @pytest.mark.asyncio
    async def test_get_total_count_no_filters(
        self, mock_connection: MagicMock, mock_cursor: AsyncMock
    ) -> None:
        """Get total count without filters."""
        mock_cursor.fetchone.return_value = (100,)

        paginator = CursorPaginator(mock_connection)
        result = await paginator._get_total_count("v_users")

        assert result == 100

    @pytest.mark.asyncio
    async def test_get_total_count_with_filters(
        self, mock_connection: MagicMock, mock_cursor: AsyncMock
    ) -> None:
        """Get total count with filters."""
        mock_cursor.fetchone.return_value = (42,)

        paginator = CursorPaginator(mock_connection)
        result = await paginator._get_total_count("v_users", filters={"status": "active"})

        assert result == 42

    @pytest.mark.asyncio
    async def test_get_total_count_returns_zero_on_none(
        self, mock_connection: MagicMock, mock_cursor: AsyncMock
    ) -> None:
        """Get total count returns 0 when result is None."""
        mock_cursor.fetchone.return_value = None

        paginator = CursorPaginator(mock_connection)
        result = await paginator._get_total_count("v_users")

        assert result == 0


# Tests for paginate_query function
@pytest.mark.unit
class TestPaginateQuery:
    """Tests for paginate_query function."""

    @pytest.mark.asyncio
    async def test_paginate_query_creates_params(
        self, mock_connection: MagicMock, mock_cursor: AsyncMock
    ) -> None:
        """paginate_query creates PaginationParams correctly."""
        mock_cursor.fetchall.return_value = []
        mock_cursor.fetchone.return_value = (0,)

        # Create a mock repository
        class MockRepository:
            connection = mock_connection

        repo = MockRepository()
        result = await paginate_query(repo, "v_users", first=5)  # type: ignore[arg-type]

        assert "edges" in result
        assert "page_info" in result

    @pytest.mark.asyncio
    async def test_paginate_query_with_all_params(
        self, mock_connection: MagicMock, mock_cursor: AsyncMock
    ) -> None:
        """paginate_query passes all parameters correctly."""
        mock_cursor.fetchall.return_value = []
        mock_cursor.fetchone.return_value = (0,)

        class MockRepository:
            connection = mock_connection

        repo = MockRepository()
        cursor = encode_cursor("test")
        result = await paginate_query(
            repo,  # type: ignore[arg-type]
            "v_users",
            first=10,
            after=cursor,
            filters={"status": "active"},
            order_by="created_at",
            order_direction="DESC",
            include_total=True,
        )

        assert result is not None


# Tests for edge cases
@pytest.mark.unit
class TestEdgeCases:
    """Tests for edge cases."""

    @pytest.mark.asyncio
    async def test_paginate_desc_order(
        self, mock_connection: MagicMock, mock_cursor: AsyncMock
    ) -> None:
        """Paginate with DESC order direction."""
        mock_cursor.fetchall.return_value = [
            (uuid4(), {"id": "2", "name": "B"}),
            (uuid4(), {"id": "1", "name": "A"}),
        ]
        mock_cursor.fetchone.return_value = (2,)

        paginator = CursorPaginator(mock_connection)
        params = PaginationParams(first=10, order_direction="DESC")
        result = await paginator.paginate("v_users", params)

        assert len(result["edges"]) == 2

    @pytest.mark.asyncio
    async def test_paginate_custom_order_by_field(
        self, mock_connection: MagicMock, mock_cursor: AsyncMock
    ) -> None:
        """Paginate with custom order_by field."""
        mock_cursor.fetchall.return_value = [
            (uuid4(), {"created_at": "2024-01-02", "name": "B"}),
            (uuid4(), {"created_at": "2024-01-01", "name": "A"}),
        ]
        mock_cursor.fetchone.return_value = (2,)

        paginator = CursorPaginator(mock_connection)
        params = PaginationParams(first=10, order_by="created_at")
        result = await paginator.paginate("v_users", params)

        # Cursor should be based on created_at field
        assert len(result["edges"]) == 2
        decoded = decode_cursor(result["edges"][0]["cursor"])
        assert decoded == "2024-01-02"
