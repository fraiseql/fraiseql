"""Unit tests for TypeGenerator."""

from contextlib import asynccontextmanager
from unittest.mock import AsyncMock, MagicMock

import pytest

from fraiseql.introspection.metadata_parser import TypeAnnotation
from fraiseql.introspection.postgres_introspector import ViewMetadata
from fraiseql.introspection.type_generator import TypeGenerator
from fraiseql.introspection.type_mapper import TypeMapper


class TestTypeGenerator:
    """Test TypeGenerator functionality."""

    @pytest.fixture
    def type_mapper(self) -> None:
        """Create a TypeMapper instance."""
        return TypeMapper()

    @pytest.fixture
    def type_generator(self, type_mapper) -> None:
        """Create a TypeGenerator instance."""
        return TypeGenerator(type_mapper)

    def test_view_name_to_class_name(self, type_generator) -> None:
        """Test view name to class name conversion."""
        assert type_generator._view_name_to_class_name("v_user") == "User"
        assert type_generator._view_name_to_class_name("v_user_profile") == "UserProfile"
        assert type_generator._view_name_to_class_name("tv_machine_item") == "MachineItem"
        assert type_generator._view_name_to_class_name("v_simple") == "Simple"

    def test_infer_pg_type_from_value(self, type_generator) -> None:
        """Test PostgreSQL type inference from Python values."""
        assert type_generator._infer_pg_type_from_value(True) == "boolean"
        assert type_generator._infer_pg_type_from_value(42) == "integer"
        assert type_generator._infer_pg_type_from_value(3.14) == "double precision"
        assert type_generator._infer_pg_type_from_value("hello") == "text"
        assert type_generator._infer_pg_type_from_value({"key": "value"}) == "jsonb"
        assert type_generator._infer_pg_type_from_value([1, 2, 3]) == "jsonb"
        assert type_generator._infer_pg_type_from_value(None) == "text"  # fallback

    def test_infer_pg_type_from_uuid_string(self, type_generator) -> None:
        """Test UUID string detection."""
        from uuid import uuid4

        uuid_str = str(uuid4())
        assert type_generator._infer_pg_type_from_value(uuid_str) == "uuid"

    def test_apply_type_decorator_mock(self, type_generator) -> None:
        """Test applying @type decorator (mocked)."""

        # Create a mock class
        class MockClass:
            pass

        # Mock the fraiseql.type decorator
        from unittest.mock import MagicMock, patch

        mock_decorated_class = MagicMock()
        mock_decorator_func = MagicMock(return_value=mock_decorated_class)

        with patch("fraiseql.type") as mock_decorator:
            mock_decorator.return_value = mock_decorator_func

            result = type_generator._apply_type_decorator(
                MockClass, "public.v_test", TypeAnnotation()
            )

            # Should call the decorator factory and then the decorator
            mock_decorator.assert_called_once()
            mock_decorator_func.assert_called_once_with(MockClass)
            assert result == mock_decorated_class

    def test_register_type_mock(self, type_generator) -> None:
        """Test registering type in registry (mocked)."""

        class MockClass:
            __name__ = "TestClass"

        from unittest.mock import patch

        with patch("fraiseql.db._type_registry") as mock_registry:
            type_generator._register_type(MockClass)

            # Should register the class
            mock_registry.__setitem__.assert_called_once_with("MockClass", MockClass)

    @pytest.mark.asyncio
    async def test_introspect_jsonb_column_with_data(self, type_generator) -> None:
        """Test JSONB introspection when view has data."""
        # Mock database connection (psycopg style)
        mock_conn = AsyncMock()

        # Mock check for 'data' column (should return non-None to indicate it exists)
        check_result = AsyncMock()
        check_result.fetchone = AsyncMock(return_value=("data",))

        # Mock data query result
        data_result = AsyncMock()
        mock_data = {
            "id": "550e8400-e29b-41d4-a716-446655440000",
            "name": "Test User",
            "age": 25,
            "active": True,
        }
        data_result.fetchone = AsyncMock(return_value=(mock_data,))

        # execute() is called twice: first for column check, then for data query
        mock_conn.execute = AsyncMock(side_effect=[check_result, data_result])

        @asynccontextmanager
        async def mock_connection() -> None:
            yield mock_conn

        mock_pool = MagicMock()
        mock_pool.connection = mock_connection

        result = await type_generator._introspect_jsonb_column("v_test", "public", mock_pool)

        # Should return field mappings
        assert "id" in result
        assert "name" in result
        assert "age" in result
        assert "active" in result

        assert result["id"]["type"] == "uuid"
        assert result["name"]["type"] == "text"
        assert result["age"]["type"] == "integer"
        assert result["active"]["type"] == "boolean"

    @pytest.mark.asyncio
    async def test_introspect_jsonb_column_empty_view(self, type_generator) -> None:
        """Test JSONB introspection when view is empty."""
        # Mock database connection - no data (psycopg style)
        mock_conn = AsyncMock()

        # Mock check for 'data' column (exists)
        check_result = AsyncMock()
        check_result.fetchone = AsyncMock(return_value=("data",))

        # Mock data query - empty result
        data_result = AsyncMock()
        data_result.fetchone = AsyncMock(return_value=None)  # Empty view

        # execute() is called twice: first for column check, then for data query
        mock_conn.execute = AsyncMock(side_effect=[check_result, data_result])

        # Mock the fallback introspection
        from unittest.mock import patch

        with patch.object(
            type_generator,
            "_introspect_view_definition",
            return_value={"id": {"type": "uuid", "nullable": False}},
        ) as mock_fallback:

            @asynccontextmanager
            async def mock_connection() -> None:
                yield mock_conn

            mock_pool = MagicMock()
            mock_pool.connection = mock_connection

            result = await type_generator._introspect_jsonb_column("v_test", "public", mock_pool)

            # Should call fallback
            mock_fallback.assert_called_once()
            assert result == {"id": {"type": "uuid", "nullable": False}}

    @pytest.mark.asyncio
    async def test_introspect_view_definition(self, type_generator) -> None:
        """Test view definition introspection fallback."""
        mock_conn = AsyncMock()

        # Mock result (psycopg style)
        result_mock = AsyncMock()
        mock_rows = [
            ("id", "uuid", "NO"),
            ("name", "text", "YES"),
            ("created_at", "timestamp", "NO"),
        ]
        result_mock.fetchall = AsyncMock(return_value=mock_rows)
        mock_conn.execute = AsyncMock(return_value=result_mock)

        result = await type_generator._introspect_view_definition("v_test", "public", mock_conn)

        # Should return field mappings from information_schema
        assert "id" in result
        assert "name" in result
        assert "created_at" in result

        assert result["id"]["type"] == "uuid"
        assert result["id"]["nullable"] is False
        assert result["name"]["type"] == "text"
        assert result["name"]["nullable"] is True

    @pytest.mark.asyncio
    async def test_view_comment_used_as_type_description(self, type_generator) -> None:
        """Test that PostgreSQL view comments become GraphQL type descriptions."""
        # Arrange
        view_metadata = ViewMetadata(
            schema_name="public",
            view_name="v_user",
            definition="SELECT * FROM users",
            comment="User profile data with contact information",  # PostgreSQL comment
            columns={},
        )
        annotation = TypeAnnotation()  # No explicit description

        # Mock database pool and connection
        mock_conn = AsyncMock()
        mock_row = MagicMock()
        mock_row.__getitem__.return_value = {
            "id": "550e8400-e29b-41d4-a716-446655440000",
            "name": "Test User",
        }
        mock_conn.fetchrow.return_value = mock_row

        @asynccontextmanager
        async def mock_connection() -> None:
            yield mock_conn

        mock_pool = MagicMock()
        mock_pool.connection = mock_connection

        # Act
        cls = await type_generator.generate_type_class(view_metadata, annotation, mock_pool)

        # Assert
        assert cls.__doc__ == "User profile data with contact information"
