"""Unit tests for PostgresIntrospector."""

from unittest.mock import AsyncMock, MagicMock

import pytest

from fraiseql.introspection.postgres_introspector import (
    PostgresIntrospector,
)


class TestPostgresIntrospector:
    """Test PostgresIntrospector functionality."""

    @pytest.fixture
    def mock_pool(self):
        """Create a mock connection pool."""
        pool = MagicMock()
        conn = MagicMock()
        pool.connection.return_value.__aenter__ = AsyncMock(return_value=conn)
        pool.connection.return_value.__aexit__ = AsyncMock(return_value=None)
        return pool

    @pytest.fixture
    def introspector(self, mock_pool):
        """Create PostgresIntrospector instance."""
        return PostgresIntrospector(mock_pool)

    def test_parse_function_arguments_empty(self, introspector):
        """Test parsing empty function arguments."""
        result = introspector._parse_function_arguments("")
        assert result == []

        result = introspector._parse_function_arguments("   ")
        assert result == []

    def test_parse_function_arguments_simple(self, introspector):
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

    def test_parse_function_arguments_with_defaults(self, introspector):
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

    def test_parse_function_arguments_malformed(self, introspector):
        """Test parsing malformed function arguments."""
        args_str = "malformed, p_name text"
        result = introspector._parse_function_arguments(args_str)

        # Should skip malformed arguments
        assert len(result) == 1
        assert result[0].name == "p_name"
        assert result[0].pg_type == "text"
