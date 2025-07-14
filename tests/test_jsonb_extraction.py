"""Tests for JSONB extraction functionality in production mode."""

from unittest.mock import AsyncMock, Mock

import pytest

from fraiseql import fraise_type
from fraiseql.db import FraiseQLRepository, register_type_for_view
from fraiseql.fastapi.config import FraiseQLConfig


@fraise_type(sql_source="test_view", jsonb_column="data")
class TestType:
    """Test type with explicit JSONB column."""

    id: str
    name: str
    value: int


@fraise_type(sql_source="custom_view", jsonb_column="custom_data")
class CustomTestType:
    """Test type with custom JSONB column."""

    id: str
    title: str
    content: str


@fraise_type(sql_source="auto_detect_view")
class AutoDetectType:
    """Test type that should auto-detect JSONB column."""

    id: str
    description: str


class TestJSONBExtraction:
    """Test JSONB extraction functionality."""

    def setup_method(self):
        """Set up test fixtures."""
        # Register types for testing
        register_type_for_view("test_view", TestType)
        register_type_for_view("custom_view", CustomTestType)
        register_type_for_view("auto_detect_view", AutoDetectType)

    @pytest.mark.asyncio
    async def test_explicit_jsonb_column_extraction(self):
        """Test extraction with explicitly configured JSONB column."""
        # Create mock pool and repository
        mock_pool = Mock()
        config = FraiseQLConfig(
            database_url="postgresql://test@localhost/test",
            jsonb_extraction_enabled=True,
        )
        context = {"config": config}
        repo = FraiseQLRepository(mock_pool, context)
        repo.mode = "production"

        # Mock database response with JSONB data
        mock_rows = [
            {
                "id": "123",
                "tenant_id": "tenant-456",
                "data": {"id": "123", "name": "Test Item", "value": 42},
                "created_at": "2025-01-01T00:00:00Z",
            },
            {
                "id": "124",
                "tenant_id": "tenant-456",
                "data": {"id": "124", "name": "Another Item", "value": 84},
                "created_at": "2025-01-02T00:00:00Z",
            },
        ]

        # Mock the run method to return our test data
        repo.run = AsyncMock(return_value=mock_rows)

        # Test find method
        results = await repo.find("test_view")

        # Should extract JSONB data from 'data' column
        assert len(results) == 2
        assert results[0] == {"id": "123", "name": "Test Item", "value": 42}
        assert results[1] == {"id": "124", "name": "Another Item", "value": 84}

    @pytest.mark.asyncio
    async def test_custom_jsonb_column_extraction(self):
        """Test extraction with custom JSONB column name."""
        mock_pool = Mock()
        config = FraiseQLConfig(
            database_url="postgresql://test@localhost/test",
            jsonb_extraction_enabled=True,
        )
        context = {"config": config}
        repo = FraiseQLRepository(mock_pool, context)
        repo.mode = "production"

        # Mock database response with custom JSONB column
        mock_rows = [
            {
                "id": "123",
                "tenant_id": "tenant-456",
                "custom_data": {
                    "id": "123",
                    "title": "Test Article",
                    "content": "This is test content",
                },
                "metadata": {"version": 1},
            }
        ]

        repo.run = AsyncMock(return_value=mock_rows)

        # Test find method
        results = await repo.find("custom_view")

        # Should extract JSONB data from 'custom_data' column
        assert len(results) == 1
        assert results[0] == {
            "id": "123",
            "title": "Test Article",
            "content": "This is test content",
        }

    @pytest.mark.asyncio
    async def test_default_column_detection(self):
        """Test detection of default JSONB column names."""
        mock_pool = Mock()
        config = FraiseQLConfig(
            database_url="postgresql://test@localhost/test",
            jsonb_extraction_enabled=True,
            jsonb_default_columns=["data", "json_data", "jsonb_data"],
        )
        context = {"config": config}
        repo = FraiseQLRepository(mock_pool, context)
        repo.mode = "production"

        # Mock database response with json_data column
        mock_rows = [
            {
                "id": "123",
                "json_data": {"id": "123", "description": "Auto-detected content"},
                "other_field": "ignored",
            }
        ]

        repo.run = AsyncMock(return_value=mock_rows)

        # Test find method
        results = await repo.find("auto_detect_view")

        # Should extract JSONB data from 'json_data' column
        assert len(results) == 1
        assert results[0] == {"id": "123", "description": "Auto-detected content"}

    @pytest.mark.asyncio
    async def test_auto_detect_jsonb_column(self):
        """Test auto-detection of JSONB columns by content."""
        mock_pool = Mock()
        config = FraiseQLConfig(
            database_url="postgresql://test@localhost/test",
            jsonb_extraction_enabled=True,
            jsonb_auto_detect=True,
        )
        context = {"config": config}
        repo = FraiseQLRepository(mock_pool, context)
        repo.mode = "production"

        # Mock database response with auto-detectable JSONB column
        mock_rows = [
            {
                "id": "123",
                "tenant_id": "tenant-456",
                "content_info": {  # This should be auto-detected
                    "id": "123",
                    "description": "Auto-detected from content_info",
                },
                "last_updated": "2025-01-01T00:00:00Z",
            }
        ]

        repo.run = AsyncMock(return_value=mock_rows)

        # Test find method with unregistered view (should auto-detect)
        results = await repo.find("unregistered_view")

        # Should extract JSONB data from auto-detected 'content_info' column
        assert len(results) == 1
        assert results[0] == {"id": "123", "description": "Auto-detected from content_info"}

    @pytest.mark.asyncio
    async def test_jsonb_extraction_disabled(self):
        """Test that JSONB extraction can be disabled."""
        mock_pool = Mock()
        config = FraiseQLConfig(
            database_url="postgresql://test@localhost/test", jsonb_extraction_enabled=False
        )
        context = {"config": config}
        repo = FraiseQLRepository(mock_pool, context)
        repo.mode = "production"

        # Mock database response with JSONB data
        mock_rows = [
            {"id": "123", "data": {"id": "123", "name": "Test Item"}, "tenant_id": "tenant-456"}
        ]

        repo.run = AsyncMock(return_value=mock_rows)

        # Test find method
        results = await repo.find("test_view")

        # Should return raw rows (no extraction)
        assert len(results) == 1
        assert results[0] == {
            "id": "123",
            "data": {"id": "123", "name": "Test Item"},
            "tenant_id": "tenant-456",
        }

    @pytest.mark.asyncio
    async def test_find_one_jsonb_extraction(self):
        """Test JSONB extraction in find_one method."""
        mock_pool = Mock()
        config = FraiseQLConfig(
            database_url="postgresql://test@localhost/test", jsonb_extraction_enabled=True
        )
        context = {"config": config}
        repo = FraiseQLRepository(mock_pool, context)
        repo.mode = "production"

        # Test data with JSONB column
        test_row = {
            "id": "123",
            "data": {"id": "123", "name": "Single Item", "value": 99},
            "metadata": {"version": 1},
        }

        # Mock the internal database execution to avoid complex connection mocking
        # We'll patch the _determine_jsonb_column and directly test the logic
        original_determine_jsonb = repo._determine_jsonb_column

        def mock_determine_jsonb(view_name, rows):
            if rows and "data" in rows[0]:
                return "data"
            return original_determine_jsonb(view_name, rows)

        repo._determine_jsonb_column = mock_determine_jsonb

        # Mock the connection/cursor operations used in find_one
        mock_conn = AsyncMock()
        mock_cursor = AsyncMock()
        mock_cursor.fetchone.return_value = test_row

        # Create proper async context managers
        async def cursor_context():
            return mock_cursor

        async def conn_context():
            return mock_conn

        mock_cursor_context = AsyncMock()
        mock_cursor_context.__aenter__ = AsyncMock(return_value=mock_cursor)
        mock_cursor_context.__aexit__ = AsyncMock(return_value=None)

        mock_conn_context = AsyncMock()
        mock_conn_context.__aenter__ = AsyncMock(return_value=mock_conn)
        mock_conn_context.__aexit__ = AsyncMock(return_value=None)

        mock_conn.cursor = Mock(return_value=mock_cursor_context)
        mock_pool.connection = Mock(return_value=mock_conn_context)

        # Test find_one method
        result = await repo.find_one("test_view")

        # Should extract JSONB data from the "data" column
        assert result == {"id": "123", "name": "Single Item", "value": 99}

    @pytest.mark.asyncio
    async def test_no_jsonb_column_found(self):
        """Test behavior when no JSONB column is found."""
        mock_pool = Mock()
        config = FraiseQLConfig(
            database_url="postgresql://test@localhost/test", jsonb_extraction_enabled=True
        )
        context = {"config": config}
        repo = FraiseQLRepository(mock_pool, context)
        repo.mode = "production"

        # Mock database response with no JSONB columns
        mock_rows = [{"id": "123", "name": "Plain Row", "value": 42, "tenant_id": "tenant-456"}]

        repo.run = AsyncMock(return_value=mock_rows)

        # Test find method
        results = await repo.find("plain_view")

        # Should return raw rows (no extraction possible)
        assert len(results) == 1
        assert results[0] == {
            "id": "123",
            "name": "Plain Row",
            "value": 42,
            "tenant_id": "tenant-456",
        }

    @pytest.mark.asyncio
    async def test_invalid_jsonb_column_specified(self):
        """Test behavior when specified JSONB column doesn't exist."""

        # Create a type with non-existent JSONB column
        @fraise_type(sql_source="invalid_view", jsonb_column="nonexistent_column")
        class InvalidType:
            id: str
            name: str

        register_type_for_view("invalid_view", InvalidType)

        mock_pool = Mock()
        config = FraiseQLConfig(
            database_url="postgresql://test@localhost/test", jsonb_extraction_enabled=True
        )
        context = {"config": config}
        repo = FraiseQLRepository(mock_pool, context)
        repo.mode = "production"

        # Mock database response without the specified JSONB column
        mock_rows = [
            {"id": "123", "data": {"id": "123", "name": "Test Item"}, "tenant_id": "tenant-456"}
        ]

        repo.run = AsyncMock(return_value=mock_rows)

        # Test find method - should fall back to default detection
        results = await repo.find("invalid_view")

        # Should fall back to 'data' column detection
        assert len(results) == 1
        assert results[0] == {"id": "123", "name": "Test Item"}

    @pytest.mark.asyncio
    async def test_empty_results_handling(self):
        """Test JSONB extraction with empty result sets."""
        mock_pool = Mock()
        config = FraiseQLConfig(
            database_url="postgresql://test@localhost/test", jsonb_extraction_enabled=True
        )
        context = {"config": config}
        repo = FraiseQLRepository(mock_pool, context)
        repo.mode = "production"

        # Mock empty database response
        repo.run = AsyncMock(return_value=[])

        # Test find method
        results = await repo.find("test_view")

        # Should return empty list
        assert results == []

    @pytest.mark.asyncio
    async def test_custom_default_columns_config(self):
        """Test custom default JSONB column configuration."""
        mock_pool = Mock()
        config = FraiseQLConfig(
            database_url="postgresql://test@localhost/test",
            jsonb_extraction_enabled=True,
            jsonb_default_columns=["content", "payload", "body"],
        )
        context = {"config": config}
        repo = FraiseQLRepository(mock_pool, context)
        repo.mode = "production"

        # Mock database response with custom column name
        mock_rows = [
            {
                "id": "123",
                "payload": {"id": "123", "description": "Found in payload column"},
                "metadata": {"timestamp": "2025-01-01"},
            }
        ]

        repo.run = AsyncMock(return_value=mock_rows)

        # Test find method
        results = await repo.find("custom_config_view")

        # Should extract from 'payload' column
        assert len(results) == 1
        assert results[0] == {"id": "123", "description": "Found in payload column"}

    @pytest.mark.asyncio
    async def test_auto_detect_disabled(self):
        """Test that auto-detection can be disabled."""
        mock_pool = Mock()
        config = FraiseQLConfig(
            database_url="postgresql://test@localhost/test",
            jsonb_extraction_enabled=True,
            jsonb_auto_detect=False,
        )
        context = {"config": config}
        repo = FraiseQLRepository(mock_pool, context)
        repo.mode = "production"

        # Mock database response that would normally be auto-detected
        mock_rows = [
            {
                "id": "123",
                "random_column": {
                    "id": "123",
                    "should_not_be_detected": "because auto-detect is disabled",
                },
                "other_field": "value",
            }
        ]

        repo.run = AsyncMock(return_value=mock_rows)

        # Test find method
        results = await repo.find("no_auto_detect_view")

        # Should return raw rows (no auto-detection)
        assert len(results) == 1
        assert results[0] == {
            "id": "123",
            "random_column": {
                "id": "123",
                "should_not_be_detected": "because auto-detect is disabled",
            },
            "other_field": "value",
        }

    def test_determine_jsonb_column_edge_cases(self):
        """Test edge cases in JSONB column determination."""
        mock_pool = Mock()
        config = FraiseQLConfig(
            database_url="postgresql://test@localhost/test", jsonb_extraction_enabled=True
        )
        context = {"config": config}
        repo = FraiseQLRepository(mock_pool, context)

        # Test with empty rows
        result = repo._determine_jsonb_column("test_view", [])
        assert result is None

        # Test with rows containing only excluded columns
        rows_with_excluded = [
            {
                "id": "123",
                "metadata": {"excluded": True},
                "context": {"also": "excluded"},
                "config": {"still": "excluded"},
                "user_id": "foreign_key_excluded",
            }
        ]
        result = repo._determine_jsonb_column("unknown_view", rows_with_excluded)
        assert result is None

        # Test with rows containing primitive JSONB-like column
        rows_with_primitive = [
            {
                "id": "123",
                "data": "string_not_dict",  # Should not be detected
                "value": 42,
            }
        ]
        result = repo._determine_jsonb_column("unknown_view", rows_with_primitive)
        assert result is None


class TestJSONBExtractionIntegration:
    """Integration tests for JSONB extraction with real-like scenarios."""

    @pytest.mark.asyncio
    async def test_printoptim_like_scenario(self):
        """Test scenario similar to PrintOptim's use case."""

        # Define a Machine type like PrintOptim uses
        @fraise_type(sql_source="tv_machine", jsonb_column="data")
        class Machine:
            id: str
            identifier: str
            machine_serial_number: str
            model: dict
            order: dict

        register_type_for_view("tv_machine", Machine)

        mock_pool = Mock()
        config = FraiseQLConfig(
            database_url="postgresql://test@localhost/test",
            environment="production",
            jsonb_extraction_enabled=True,
        )
        context = {"config": config}
        repo = FraiseQLRepository(mock_pool, context)
        repo.mode = "production"

        # Mock PrintOptim-like database response
        mock_rows = [
            {
                "id": "123-456-789",
                "tenant_id": "550e8400-e29b-41d4-a716-446655440000",
                "fk_customer_org": "org-123",
                "fk_provider_org": None,
                "machine_serial_number": "SN-12345",
                "data": {
                    "id": "123-456-789",
                    "identifier": "machine-001",
                    "machine_serial_number": "SN-12345",
                    "model": {"name": "Model X", "version": "1.0"},
                    "order": {"id": "order-456", "date": "2025-01-01"},
                },
                "last_updated": "2025-07-12 13:17:25",
                "updated_by": None,
            }
        ]

        repo.run = AsyncMock(return_value=mock_rows)

        # Test find method
        results = await repo.find("tv_machine")

        # Should return extracted JSONB data matching GraphQL expectations
        expected = {
            "id": "123-456-789",
            "identifier": "machine-001",
            "machine_serial_number": "SN-12345",
            "model": {"name": "Model X", "version": "1.0"},
            "order": {"id": "order-456", "date": "2025-01-01"},
        }

        assert len(results) == 1
        assert results[0] == expected
