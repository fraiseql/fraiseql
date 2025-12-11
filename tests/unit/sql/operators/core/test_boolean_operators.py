"""Tests for boolean operator strategy."""

import pytest
from psycopg.sql import Identifier

from fraiseql.sql.operators.core.boolean_operators import BooleanOperatorStrategy


class TestBooleanOperatorStrategy:
    """Test boolean operator strategy."""

    @pytest.fixture
    def strategy(self):
        """Create boolean operator strategy instance."""
        return BooleanOperatorStrategy()

    @pytest.fixture
    def path_sql(self):
        """Standard path SQL for testing."""
        return Identifier("field")

    def test_does_not_support_operators_without_type(self, strategy):
        """Test that boolean operators require type hints."""
        assert not strategy.supports_operator("eq", None)
        assert not strategy.supports_operator("neq", None)
        assert not strategy.supports_operator("isnull", None)

    def test_supports_operators_with_bool_type(self, strategy):
        """Test that operators are supported for bool fields."""
        assert strategy.supports_operator("eq", bool)
        assert strategy.supports_operator("neq", bool)
        assert strategy.supports_operator("isnull", bool)

    def test_does_not_support_non_bool_types(self, strategy):
        """Test that non-boolean types are not supported."""
        assert not strategy.supports_operator("eq", str)
        assert not strategy.supports_operator("eq", int)

    def test_does_not_support_unsupported_operators(self, strategy):
        """Test that unsupported operators are rejected."""
        assert not strategy.supports_operator("gt", bool)
        assert not strategy.supports_operator("contains", bool)
        assert not strategy.supports_operator("in", bool)

    def test_equality_operators(self, strategy, path_sql):
        """Test equality operators."""
        sql = strategy.build_sql("eq", True, path_sql, field_type=bool)
        assert sql is not None
        assert "=" in sql.as_string(None)
        assert "true" in sql.as_string(None).lower()

        sql = strategy.build_sql("eq", False, path_sql, field_type=bool)
        assert sql is not None
        assert "false" in sql.as_string(None).lower()

        sql = strategy.build_sql("neq", True, path_sql, field_type=bool)
        assert sql is not None
        assert "!=" in sql.as_string(None)

    def test_isnull_operator(self, strategy, path_sql):
        """Test NULL checking."""
        sql = strategy.build_sql("isnull", True, path_sql, field_type=bool)
        assert sql is not None
        assert "IS NULL" in sql.as_string(None)

        sql = strategy.build_sql("isnull", False, path_sql, field_type=bool)
        assert sql is not None
        assert "IS NOT NULL" in sql.as_string(None)

    def test_jsonb_casting(self, strategy, path_sql):
        """Test that JSONB fields are cast to boolean."""
        sql = strategy.build_sql("eq", True, path_sql, field_type=bool, jsonb_column="data")
        assert sql is not None
        assert "::boolean" in sql.as_string(None)

    def test_unsupported_operator_returns_none(self, strategy, path_sql):
        """Test that unsupported operators return None."""
        sql = strategy.build_sql("unknown_op", True, path_sql, field_type=bool)
        assert sql is None
