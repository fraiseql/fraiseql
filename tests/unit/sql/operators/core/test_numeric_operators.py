"""Tests for numeric operator strategy."""

import pytest
from psycopg.sql import Identifier

from fraiseql.sql.operators.core.numeric_operators import NumericOperatorStrategy


class TestNumericOperatorStrategy:
    """Test numeric operator strategy."""

    @pytest.fixture
    def strategy(self):
        """Create numeric operator strategy instance."""
        return NumericOperatorStrategy()

    @pytest.fixture
    def path_sql(self):
        """Standard path SQL for testing."""
        return Identifier("field")

    def test_supports_numeric_specific_operators_without_type(self, strategy):
        """Test that numeric-specific operators are supported without type hint."""
        assert strategy.supports_operator("gt", None)
        assert strategy.supports_operator("gte", None)
        assert strategy.supports_operator("lt", None)
        assert strategy.supports_operator("lte", None)

    def test_does_not_support_generic_operators_without_type(self, strategy):
        """Test that generic operators need type hints."""
        assert not strategy.supports_operator("eq", None)
        assert not strategy.supports_operator("neq", None)
        assert not strategy.supports_operator("in", None)

    def test_supports_all_operators_with_int_type(self, strategy):
        """Test that all operators are supported for int fields."""
        assert strategy.supports_operator("eq", int)
        assert strategy.supports_operator("neq", int)
        assert strategy.supports_operator("gt", int)
        assert strategy.supports_operator("gte", int)
        assert strategy.supports_operator("lt", int)
        assert strategy.supports_operator("lte", int)
        assert strategy.supports_operator("in", int)
        assert strategy.supports_operator("isnull", int)

    def test_supports_all_operators_with_float_type(self, strategy):
        """Test that all operators are supported for float fields."""
        assert strategy.supports_operator("eq", float)
        assert strategy.supports_operator("gt", float)
        assert strategy.supports_operator("lt", float)

    def test_does_not_support_non_numeric_types(self, strategy):
        """Test that non-numeric types are not supported."""
        assert not strategy.supports_operator("eq", str)
        # Note: bool is a subclass of int in Python, so it will match numeric types
        # This is expected behavior

    def test_equality_operators(self, strategy, path_sql):
        """Test equality operators."""
        sql = strategy.build_sql("eq", 42, path_sql, field_type=int)
        assert sql is not None
        assert "=" in sql.as_string(None)

        sql = strategy.build_sql("neq", 42, path_sql, field_type=int)
        assert sql is not None
        assert "!=" in sql.as_string(None)

    def test_comparison_operators(self, strategy, path_sql):
        """Test comparison operators."""
        sql = strategy.build_sql("gt", 100, path_sql, field_type=int)
        assert sql is not None
        assert ">" in sql.as_string(None)

        sql = strategy.build_sql("gte", 100, path_sql, field_type=int)
        assert sql is not None
        assert ">=" in sql.as_string(None)

        sql = strategy.build_sql("lt", 100, path_sql, field_type=int)
        assert sql is not None
        assert "<" in sql.as_string(None)

        sql = strategy.build_sql("lte", 100, path_sql, field_type=int)
        assert sql is not None
        assert "<=" in sql.as_string(None)

    def test_in_operators(self, strategy, path_sql):
        """Test IN operators."""
        sql = strategy.build_sql("in", [1, 2, 3], path_sql, field_type=int)
        assert sql is not None
        assert "IN" in sql.as_string(None)

        sql = strategy.build_sql("nin", [1, 2], path_sql, field_type=int)
        assert sql is not None
        assert "NOT IN" in sql.as_string(None)

    def test_isnull_operator(self, strategy, path_sql):
        """Test NULL checking."""
        sql = strategy.build_sql("isnull", True, path_sql, field_type=int)
        assert sql is not None
        assert "IS NULL" in sql.as_string(None)

        sql = strategy.build_sql("isnull", False, path_sql, field_type=int)
        assert sql is not None
        assert "IS NOT NULL" in sql.as_string(None)

    def test_jsonb_casting_for_integers(self, strategy, path_sql):
        """Test that JSONB fields are cast to integer."""
        sql = strategy.build_sql("eq", 42, path_sql, field_type=int, jsonb_column="data")
        assert sql is not None
        assert "::integer" in sql.as_string(None)

    def test_jsonb_casting_for_floats(self, strategy, path_sql):
        """Test that JSONB fields are cast to numeric for floats."""
        sql = strategy.build_sql("eq", 3.14, path_sql, field_type=float, jsonb_column="data")
        assert sql is not None
        assert "::numeric" in sql.as_string(None)

    def test_unsupported_operator_returns_none(self, strategy, path_sql):
        """Test that unsupported operators return None."""
        sql = strategy.build_sql("unknown_op", 42, path_sql, field_type=int)
        assert sql is None
