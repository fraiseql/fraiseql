"""Tests for JSONB extraction functionality in production mode."""

from unittest.mock import AsyncMock

import pytest

import fraiseql
from fraiseql.db import FraiseQLRepository, register_type_for_view
from fraiseql.fastapi.config import FraiseQLConfig


@fraiseql.type(sql_source="test_view", jsonb_column="data")
class SampleType:
    """Test type with explicit JSONB column."""

    id: str
    name: str
    value: int


@fraiseql.type(sql_source="auto_detect_view")
class AutoDetectType:
    """Test type that should auto-detect JSONB column."""

    id: str
    description: str


class TestJSONBExtraction:
    """Test JSONB extraction functionality."""

    def setup_method(self):
        """Set up test fixtures."""
        # Register types for testing
        register_type_for_view("test_view", SampleType)
        register_type_for_view("auto_detect_view", AutoDetectType)

    @pytest.mark.asyncio
    async def test_explicit_jsonb_column_extraction(self, db_pool):
        """Test extraction with explicitly configured JSONB column."""
        # Use a separate connection for table creation that we'll commit
        async with db_pool.connection() as setup_conn:
            # Create test view with JSONB data
            await setup_conn.execute("""
                CREATE TABLE IF NOT EXISTS test_view (
                    id TEXT PRIMARY KEY,
                    tenant_id TEXT,
                    data JSONB,
                    created_at TIMESTAMPTZ DEFAULT NOW()
                )
            """)

            # Insert test data
            await setup_conn.execute("""
                INSERT INTO test_view (id, tenant_id, data) VALUES
                ('123', 'tenant-456', '{"id": "123", "name": "Test Item", "value": 42}'::jsonb),
                ('124', 'tenant-456', '{"id": "124", "name": "Another Item", "value": 84}'::jsonb)
            """)
            await setup_conn.commit()

        try:
            # Create repository with production mode
            config = FraiseQLConfig(
                database_url="postgresql://test@localhost/test", jsonb_extraction_enabled=True
            )
            context = {"config": config}
            repo = FraiseQLRepository(db_pool, context)
            repo.mode = "production"

            # Test find method
            results = await repo.find("test_view")

            # Should extract JSONB data from 'data' column
            assert len(results) == 2
            assert results[0] == {"id": "123", "name": "Test Item", "value": 42}
            assert results[1] == {"id": "124", "name": "Another Item", "value": 84}
        finally:
            # Clean up
            async with db_pool.connection() as cleanup_conn:
                await cleanup_conn.execute("DROP TABLE IF EXISTS test_view CASCADE")
                await cleanup_conn.commit()

        # Already tested above

    @pytest.mark.asyncio
    async def test_default_column_detection(self, db_pool):
        """Test detection of default JSONB column names."""
        # Create table with json_data column (one of the default columns)
        async with db_pool.connection() as setup_conn:
            await setup_conn.execute("""
                CREATE TABLE IF NOT EXISTS auto_detect_view (
                    id TEXT PRIMARY KEY,
                    json_data JSONB,
                    other_field TEXT
                )
            """)

            await setup_conn.execute("""
                INSERT INTO auto_detect_view (id, json_data, other_field) VALUES
                ('123', '{"id": "123", "description": "Auto-detected content"}'::jsonb, 'ignored')
            """)
            await setup_conn.commit()

        try:
            config = FraiseQLConfig(
                database_url="postgresql://test@localhost/test",
                jsonb_extraction_enabled=True,
                jsonb_default_columns=["data", "json_data", "jsonb_data"],
            )
            context = {"config": config}
            repo = FraiseQLRepository(db_pool, context)
            repo.mode = "production"

            # Test find method
            results = await repo.find("auto_detect_view")

            # Should extract JSONB data from 'json_data' column
            assert len(results) == 1
            assert results[0] == {"id": "123", "description": "Auto-detected content"}
        finally:
            async with db_pool.connection() as cleanup_conn:
                await cleanup_conn.execute("DROP TABLE IF EXISTS auto_detect_view CASCADE")
                await cleanup_conn.commit()

    @pytest.mark.asyncio
    async def test_auto_detect_jsonb_column(self, db_pool):
        """Test auto-detection of JSONB columns by content."""
        # Create table with a JSONB column that should be auto-detected
        async with db_pool.connection() as setup_conn:
            await setup_conn.execute("""
                CREATE TABLE IF NOT EXISTS unregistered_view (
                    id TEXT PRIMARY KEY,
                    tenant_id TEXT,
                    content_info JSONB,
                    last_updated TIMESTAMPTZ
                )
            """)

            await setup_conn.execute("""
                INSERT INTO unregistered_view (id, tenant_id, content_info, last_updated) VALUES
                ('123', 'tenant-456',
                 '{"id": "123", "description": "Auto-detected from content_info"}'::jsonb,
                 '2025-01-01T00:00:00Z')
            """)
            await setup_conn.commit()

        try:
            config = FraiseQLConfig(
                database_url="postgresql://test@localhost/test",
                jsonb_extraction_enabled=True,
                jsonb_auto_detect=True,
            )
            context = {"config": config}
            repo = FraiseQLRepository(db_pool, context)
            repo.mode = "production"

            # Test find method with unregistered view (should auto-detect)
            results = await repo.find("unregistered_view")

            # Should extract JSONB data from auto-detected 'content_info' column
            assert len(results) == 1
            assert results[0] == {"id": "123", "description": "Auto-detected from content_info"}
        finally:
            async with db_pool.connection() as cleanup_conn:
                await cleanup_conn.execute("DROP TABLE IF EXISTS unregistered_view CASCADE")
                await cleanup_conn.commit()

    @pytest.mark.asyncio
    async def test_jsonb_extraction_disabled(self, db_pool):
        """Test that JSONB extraction can be disabled."""
        # Create table with JSONB data
        async with db_pool.connection() as setup_conn:
            await setup_conn.execute("""
                CREATE TABLE IF NOT EXISTS disabled_test_view (
                    id TEXT PRIMARY KEY,
                    data JSONB,
                    tenant_id TEXT
                )
            """)

            await setup_conn.execute("""
                INSERT INTO disabled_test_view (id, data, tenant_id) VALUES
                ('123', '{"id": "123", "name": "Test Item"}'::jsonb, 'tenant-456')
            """)
            await setup_conn.commit()

        try:
            config = FraiseQLConfig(
                database_url="postgresql://test@localhost/test", jsonb_extraction_enabled=False
            )
            context = {"config": config}
            repo = FraiseQLRepository(db_pool, context)
            repo.mode = "production"

            # Test find method
            results = await repo.find("disabled_test_view")

            # Should return raw rows (no extraction)
            assert len(results) == 1
            assert results[0] == {
                "id": "123",
                "data": {"id": "123", "name": "Test Item"},
                "tenant_id": "tenant-456",
            }
        finally:
            async with db_pool.connection() as cleanup_conn:
                await cleanup_conn.execute("DROP TABLE IF EXISTS disabled_test_view CASCADE")
                await cleanup_conn.commit()

    @pytest.mark.asyncio
    async def test_find_one_jsonb_extraction(self, db_pool):
        """Test JSONB extraction in find_one method."""
        # Create table with JSONB data
        async with db_pool.connection() as setup_conn:
            await setup_conn.execute("""
                CREATE TABLE IF NOT EXISTS find_one_test_view (
                    id TEXT PRIMARY KEY,
                    data JSONB,
                    metadata JSONB
                )
            """)

            await setup_conn.execute("""
                INSERT INTO find_one_test_view (id, data, metadata) VALUES
                ('123', '{"id": "123", "name": "Single Item", "value": 99}'::jsonb, '{"version": 1}'::jsonb)
            """)
            await setup_conn.commit()

        try:
            config = FraiseQLConfig(
                database_url="postgresql://test@localhost/test", jsonb_extraction_enabled=True
            )
            context = {"config": config}
            repo = FraiseQLRepository(db_pool, context)
            repo.mode = "production"

            # Test find_one method
            result = await repo.find_one("find_one_test_view")

            # Should extract JSONB data from the "data" column
            assert result == {"id": "123", "name": "Single Item", "value": 99}
        finally:
            async with db_pool.connection() as cleanup_conn:
                await cleanup_conn.execute("DROP TABLE IF EXISTS find_one_test_view CASCADE")
                await cleanup_conn.commit()

    @pytest.mark.asyncio
    async def test_no_jsonb_column_found(self, db_pool):
        """Test behavior when no JSONB column is found."""
        # Create table with no JSONB columns
        async with db_pool.connection() as setup_conn:
            await setup_conn.execute("""
                CREATE TABLE IF NOT EXISTS plain_view (
                    id TEXT PRIMARY KEY,
                    name TEXT,
                    value INTEGER,
                    tenant_id TEXT
                )
            """)

            await setup_conn.execute("""
                INSERT INTO plain_view (id, name, value, tenant_id) VALUES
                ('123', 'Plain Row', 42, 'tenant-456')
            """)
            await setup_conn.commit()

        try:
            config = FraiseQLConfig(
                database_url="postgresql://test@localhost/test", jsonb_extraction_enabled=True
            )
            context = {"config": config}
            repo = FraiseQLRepository(db_pool, context)
            repo.mode = "production"

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
        finally:
            async with db_pool.connection() as cleanup_conn:
                await cleanup_conn.execute("DROP TABLE IF EXISTS plain_view CASCADE")
                await cleanup_conn.commit()

    @pytest.mark.asyncio
    async def test_invalid_jsonb_column_specified(self, db_pool):
        """Test behavior when specified JSONB column doesn't exist."""

        # Create a type with non-existent JSONB column
        @fraiseql.type(sql_source="invalid_view", jsonb_column="nonexistent_column")
        class InvalidType:
            id: str
            name: str

        register_type_for_view("invalid_view", InvalidType)

        # Create table without the specified JSONB column but with a 'data' column
        async with db_pool.connection() as setup_conn:
            await setup_conn.execute("""
                CREATE TABLE IF NOT EXISTS invalid_view (
                    id TEXT PRIMARY KEY,
                    data JSONB,
                    tenant_id TEXT
                )
            """)

            await setup_conn.execute("""
                INSERT INTO invalid_view (id, data, tenant_id) VALUES
                ('123', '{"id": "123", "name": "Test Item"}'::jsonb, 'tenant-456')
            """)
            await setup_conn.commit()

        try:
            config = FraiseQLConfig(
                database_url="postgresql://test@localhost/test", jsonb_extraction_enabled=True
            )
            context = {"config": config}
            repo = FraiseQLRepository(db_pool, context)
            repo.mode = "production"

            # Test find method - should fall back to default detection
            results = await repo.find("invalid_view")

            # Should fall back to 'data' column detection
            assert len(results) == 1
            assert results[0] == {"id": "123", "name": "Test Item"}
        finally:
            async with db_pool.connection() as cleanup_conn:
                await cleanup_conn.execute("DROP TABLE IF EXISTS invalid_view CASCADE")
                await cleanup_conn.commit()

    @pytest.mark.asyncio
    async def test_empty_results_handling(self, db_pool):
        """Test JSONB extraction with empty result sets."""
        # Create empty table
        async with db_pool.connection() as setup_conn:
            await setup_conn.execute("""
                CREATE TABLE IF NOT EXISTS empty_view (
                    id TEXT PRIMARY KEY,
                    data JSONB
                )
            """)
            await setup_conn.commit()

        try:
            config = FraiseQLConfig(
                database_url="postgresql://test@localhost/test", jsonb_extraction_enabled=True
            )
            context = {"config": config}
            repo = FraiseQLRepository(db_pool, context)
            repo.mode = "production"

            # Test find method
            results = await repo.find("empty_view")

            # Should return empty list
            assert results == []
        finally:
            async with db_pool.connection() as cleanup_conn:
                await cleanup_conn.execute("DROP TABLE IF EXISTS empty_view CASCADE")
                await cleanup_conn.commit()

    @pytest.mark.asyncio
    async def test_custom_default_columns_config(self, db_pool):
        """Test custom default JSONB column configuration."""
        # Create table with custom JSONB column name
        async with db_pool.connection() as setup_conn:
            await setup_conn.execute("""
                CREATE TABLE IF NOT EXISTS custom_config_view (
                    id TEXT PRIMARY KEY,
                    payload JSONB,
                    metadata JSONB
                )
            """)

            await setup_conn.execute("""
                INSERT INTO custom_config_view (id, payload, metadata) VALUES
                ('123', '{"id": "123", "description": "Found in payload column"}'::jsonb, '{"timestamp": "2025-01-01"}'::jsonb)
            """)
            await setup_conn.commit()

        try:
            config = FraiseQLConfig(
                database_url="postgresql://test@localhost/test",
                jsonb_extraction_enabled=True,
                jsonb_default_columns=["content", "payload", "body"],
            )
            context = {"config": config}
            repo = FraiseQLRepository(db_pool, context)
            repo.mode = "production"

            # Test find method
            results = await repo.find("custom_config_view")

            # Should extract from 'payload' column
            assert len(results) == 1
            assert results[0] == {"id": "123", "description": "Found in payload column"}
        finally:
            async with db_pool.connection() as cleanup_conn:
                await cleanup_conn.execute("DROP TABLE IF EXISTS custom_config_view CASCADE")
                await cleanup_conn.commit()

    @pytest.mark.asyncio
    async def test_auto_detect_disabled(self, db_pool):
        """Test that auto-detection can be disabled."""
        # Create table with JSONB column that would normally be auto-detected
        async with db_pool.connection() as setup_conn:
            await setup_conn.execute("""
                CREATE TABLE IF NOT EXISTS no_auto_detect_view (
                    id TEXT PRIMARY KEY,
                    strange_column JSONB,
                    regular_field TEXT
                )
            """)

            await setup_conn.execute("""
                INSERT INTO no_auto_detect_view (id, strange_column, regular_field) VALUES
                ('123', '{"id": "123", "auto_data": "Should not be extracted"}'::jsonb, 'Normal text')
            """)
            await setup_conn.commit()

        try:
            config = FraiseQLConfig(
                database_url="postgresql://test@localhost/test",
                jsonb_extraction_enabled=True,
                jsonb_auto_detect=False,
            )
            context = {"config": config}
            repo = FraiseQLRepository(db_pool, context)
            repo.mode = "production"

            # Test find method
            results = await repo.find("no_auto_detect_view")

            # Should return raw data (no auto-detection)
            assert len(results) == 1
            # Since auto-detect is disabled and strange_column is not in default list,
            # it should return the full row
            assert results[0] == {
                "id": "123",
                "strange_column": {"id": "123", "auto_data": "Should not be extracted"},
                "regular_field": "Normal text",
            }
        finally:
            async with db_pool.connection() as cleanup_conn:
                await cleanup_conn.execute("DROP TABLE IF EXISTS no_auto_detect_view CASCADE")
                await cleanup_conn.commit()

    def test_determine_jsonb_column_edge_cases(self):
        """Test edge cases in JSONB column determination."""
        mock_pool = AsyncMock()
        mock_conn = AsyncMock()
        mock_pool.connection.return_value.__aenter__.return_value = mock_conn
        mock_pool.connection.return_value.__aexit__.return_value = None
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
    async def test_printoptim_like_scenario(self, db_pool):
        """Test scenario similar to PrintOptim's use case."""

        # Define a Machine type like PrintOptim uses
        @fraiseql.type(sql_source="tv_machine", jsonb_column="data")
        class Machine:
            id: str
            identifier: str
            machine_serial_number: str
            model: dict
            order: dict

        register_type_for_view("tv_machine", Machine)

        # Create table with PrintOptim-like structure
        async with db_pool.connection() as setup_conn:
            await setup_conn.execute("""
                CREATE TABLE IF NOT EXISTS tv_machine (
                    id TEXT PRIMARY KEY,
                    tenant_id TEXT,
                    fk_customer_org TEXT,
                    fk_provider_org TEXT,
                    machine_serial_number TEXT,
                    data JSONB,
                    last_updated TIMESTAMPTZ,
                    updated_by TEXT
                )
            """)

            await setup_conn.execute("""
                INSERT INTO tv_machine (
                    id, tenant_id, fk_customer_org, fk_provider_org,
                    machine_serial_number, data, last_updated, updated_by
                ) VALUES (
                    '123-456-789',
                    '550e8400-e29b-41d4-a716-446655440000',
                    'org-123',
                    NULL,
                    'SN-12345',
                    '{"id": "123-456-789", "identifier": "machine-001", "machine_serial_number": "SN-12345", "model": {"name": "Model X", "version": "1.0"}, "order": {"id": "order-456", "date": "2025-01-01"}}'::jsonb,
                    '2025-07-12 13:17:25',
                    NULL
                )
            """)
            await setup_conn.commit()

        try:
            config = FraiseQLConfig(
                database_url="postgresql://test@localhost/test",
                environment="production",
                jsonb_extraction_enabled=True,
            )
            context = {"config": config}
            repo = FraiseQLRepository(db_pool, context)
            repo.mode = "production"

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
        finally:
            async with db_pool.connection() as cleanup_conn:
                await cleanup_conn.execute("DROP TABLE IF EXISTS tv_machine CASCADE")
                await cleanup_conn.commit()
