"""Unit tests for PostgresIntrospector."""

from unittest.mock import AsyncMock, MagicMock

import pytest

from fraiseql.introspection.postgres_introspector import (
    PostgresIntrospector,
)


class TestPostgresIntrospector:
    """Test PostgresIntrospector functionality."""

    @pytest.fixture
    def mock_pool(self) -> None:
        """Create a mock connection pool."""
        pool = MagicMock()
        conn = MagicMock()
        pool.connection.return_value.__aenter__ = AsyncMock(return_value=conn)
        pool.connection.return_value.__aexit__ = AsyncMock(return_value=None)
        return pool

    @pytest.fixture
    def introspector(self, mock_pool) -> None:
        """Create PostgresIntrospector instance."""
        return PostgresIntrospector(mock_pool)

    def test_parse_function_arguments_empty(self, introspector) -> None:
        """Test parsing empty function arguments."""
        result = introspector._parse_function_arguments("")
        assert result == []

        result = introspector._parse_function_arguments("   ")
        assert result == []

    def test_parse_function_arguments_simple(self, introspector) -> None:
        """Test parsing simple function arguments."""
        args_str = "p_name text, p_email text"
        result = introspector._parse_function_arguments(args_str)

        assert len(result) == 2

        assert result[0].name == "p_name"
        assert result[0].pg_type == "text"
        assert result[0].mode == "IN"
        assert result[0].default_value is None

        assert result[1].name == "p_email"
        assert result[1].pg_type == "text"
        assert result[1].mode == "IN"
        assert result[1].default_value is None

    def test_parse_function_arguments_with_defaults(self, introspector) -> None:
        """Test parsing function arguments with default values."""
        args_str = "p_name text, p_email text DEFAULT 'test@example.com'"
        result = introspector._parse_function_arguments(args_str)

        assert len(result) == 2

        assert result[0].name == "p_name"
        assert result[0].pg_type == "text"
        assert result[0].default_value is None

        assert result[1].name == "p_email"
        assert result[1].pg_type == "text"
        assert result[1].default_value == "'test@example.com'"

    def test_parse_function_arguments_malformed(self, introspector) -> None:
        """Test parsing malformed function arguments."""
        args_str = "malformed, p_name text"
        result = introspector._parse_function_arguments(args_str)

        # Should skip malformed arguments
        assert len(result) == 1
        assert result[0].name == "p_name"
        assert result[0].pg_type == "text"

    @pytest.mark.asyncio
    async def test_discover_composite_type_found(self, introspector, mock_pool) -> None:
        """Test composite type discovery when type exists."""
        # Mock the database responses
        conn = mock_pool.connection.return_value.__aenter__.return_value

        # Mock type query result (returns tuple: type_name, schema_name, comment)
        type_result = MagicMock()
        type_result.fetchone = AsyncMock(
            return_value=(
                "type_create_contact_input",
                "app",
                "@fraiseql:input name=CreateContactInput",
            )
        )

        # Mock attribute query result (returns tuples: attribute_name, pg_type, ordinal_position, comment)
        attr_result = MagicMock()
        attr_result.fetchall = AsyncMock(
            return_value=[
                (
                    "email",
                    "text",
                    1,
                    "@fraiseql:field name=email,type=String!,required=true",
                ),
                (
                    "company_id",
                    "uuid",
                    2,
                    "@fraiseql:field name=companyId,type=UUID,required=false",
                ),
            ]
        )

        # Configure execute to return different results for different queries
        conn.execute = AsyncMock(side_effect=[type_result, attr_result])

        # When: Discover composite type
        result = await introspector.discover_composite_type(
            "type_create_contact_input", schema="app"
        )

        # Then: Returns metadata
        assert result is not None
        assert result.type_name == "type_create_contact_input"
        assert result.schema_name == "app"
        assert result.comment == "@fraiseql:input name=CreateContactInput"

        # Then: Has correct attributes
        assert len(result.attributes) == 2

        email_attr = result.attributes[0]
        assert email_attr.name == "email"
        assert email_attr.pg_type == "text"
        assert email_attr.ordinal_position == 1
        assert email_attr.comment == "@fraiseql:field name=email,type=String!,required=true"

        company_attr = result.attributes[1]
        assert company_attr.name == "company_id"
        assert company_attr.pg_type == "uuid"
        assert company_attr.ordinal_position == 2

    @pytest.mark.asyncio
    async def test_discover_composite_type_not_found(self, introspector, mock_pool) -> None:
        """Test composite type discovery when type doesn't exist."""
        # Mock the database response
        conn = mock_pool.connection.return_value.__aenter__.return_value

        # Mock type query result (no rows)
        type_result = MagicMock()
        type_result.fetchone = AsyncMock(return_value=None)
        conn.execute = AsyncMock(return_value=type_result)

        # When: Discover non-existent composite type
        result = await introspector.discover_composite_type("type_nonexistent_input", schema="app")

        # Then: Returns None
        assert result is None

    @pytest.mark.asyncio
    async def test_discover_views_includes_column_comments(self, introspector, mock_pool) -> None:
        """Test that view discovery includes column comments from PostgreSQL."""
        # Mock the database responses
        conn = mock_pool.connection.return_value.__aenter__.return_value

        # Mock views query result
        views_result = MagicMock()
        views_result.fetchall = AsyncMock(
            return_value=[
                ("public", "v_users", "SELECT id, email, name FROM users"),
            ]
        )

        # Mock comment query result
        comment_result = MagicMock()
        comment_result.fetchone = AsyncMock(return_value=("User profile data",))

        # Mock columns query result with column comments
        columns_result = MagicMock()
        columns_result.fetchall = AsyncMock(
            return_value=[
                ("id", "uuid", True, "Unique identifier for the user"),
                ("email", "text", False, "Primary email address"),
                ("name", "text", False, "Full name of the user"),
            ]
        )

        # Configure execute to return different results for different queries
        conn.execute = AsyncMock(side_effect=[views_result, comment_result, columns_result])

        # When: Discover views
        views = await introspector.discover_views()

        # Then: Returns view metadata with column comments
        assert len(views) == 1
        view = views[0]

        assert view.view_name == "v_users"
        assert view.comment == "User profile data"

        # Then: Columns include comments
        assert "id" in view.columns
        assert "email" in view.columns
        assert "name" in view.columns

        assert view.columns["id"].comment == "Unique identifier for the user"
        assert view.columns["email"].comment == "Primary email address"
        assert view.columns["name"].comment == "Full name of the user"
