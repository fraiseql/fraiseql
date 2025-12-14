"""Tests for string operator strategy."""

import pytest
from psycopg.sql import Identifier

from fraiseql.sql.operators.core.string_operators import StringOperatorStrategy


class TestStringOperatorStrategy:
    """Test string operator strategy."""

    @pytest.fixture
    def strategy(self):
        """Create string operator strategy instance."""
        return StringOperatorStrategy()

    @pytest.fixture
    def path_sql(self):
        """Standard path SQL for testing."""
        return Identifier("field")

    def test_supports_string_specific_operators_without_type(self, strategy):
        """Test that string-specific operators are supported without type hint."""
        assert strategy.supports_operator("contains", None)
        assert strategy.supports_operator("icontains", None)
        assert strategy.supports_operator("startswith", None)
        assert strategy.supports_operator("istartswith", None)
        assert strategy.supports_operator("endswith", None)
        assert strategy.supports_operator("iendswith", None)
        assert strategy.supports_operator("like", None)
        assert strategy.supports_operator("ilike", None)
        assert strategy.supports_operator("matches", None)

    def test_does_not_support_generic_operators_without_type(self, strategy):
        """Test that generic operators need type hints."""
        assert not strategy.supports_operator("eq", None)
        assert not strategy.supports_operator("neq", None)
        assert not strategy.supports_operator("in", None)
        assert not strategy.supports_operator("nin", None)

    def test_supports_all_operators_with_str_type(self, strategy):
        """Test that all operators are supported for str fields."""
        assert strategy.supports_operator("eq", str)
        assert strategy.supports_operator("neq", str)
        assert strategy.supports_operator("contains", str)
        assert strategy.supports_operator("in", str)
        assert strategy.supports_operator("isnull", str)

    def test_equality_operators(self, strategy, path_sql):
        """Test equality operators."""
        sql = strategy.build_sql("eq", "test", path_sql, field_type=str)
        assert sql is not None
        assert "=" in sql.as_string(None)

        sql = strategy.build_sql("neq", "test", path_sql, field_type=str)
        assert sql is not None
        assert "!=" in sql.as_string(None)

    def test_contains_operators(self, strategy, path_sql):
        """Test contains operators."""
        sql = strategy.build_sql("contains", "test", path_sql, field_type=str)
        assert sql is not None
        assert "LIKE" in sql.as_string(None)
        assert "%test%" in sql.as_string(None)

        sql = strategy.build_sql("icontains", "test", path_sql, field_type=str)
        assert sql is not None
        assert "ILIKE" in sql.as_string(None)
        assert "%test%" in sql.as_string(None)

    def test_startswith_operators(self, strategy, path_sql):
        """Test startswith operators."""
        sql = strategy.build_sql("startswith", "test", path_sql, field_type=str)
        assert sql is not None
        assert "LIKE" in sql.as_string(None)
        assert "test%" in sql.as_string(None)

        sql = strategy.build_sql("istartswith", "test", path_sql, field_type=str)
        assert sql is not None
        assert "ILIKE" in sql.as_string(None)
        assert "test%" in sql.as_string(None)

    def test_endswith_operators(self, strategy, path_sql):
        """Test endswith operators."""
        sql = strategy.build_sql("endswith", "test", path_sql, field_type=str)
        assert sql is not None
        assert "LIKE" in sql.as_string(None)
        assert "%test" in sql.as_string(None)

        sql = strategy.build_sql("iendswith", "test", path_sql, field_type=str)
        assert sql is not None
        assert "ILIKE" in sql.as_string(None)
        assert "%test" in sql.as_string(None)

    def test_like_operators(self, strategy, path_sql):
        """Test explicit LIKE operators."""
        sql = strategy.build_sql("like", "test%", path_sql, field_type=str)
        assert sql is not None
        assert "LIKE" in sql.as_string(None)
        assert "test%" in sql.as_string(None)

        sql = strategy.build_sql("ilike", "test%", path_sql, field_type=str)
        assert sql is not None
        assert "ILIKE" in sql.as_string(None)

    def test_regex_operators(self, strategy, path_sql):
        """Test regex operators."""
        sql = strategy.build_sql("matches", "^test.*", path_sql, field_type=str)
        assert sql is not None
        assert "~" in sql.as_string(None)

        sql = strategy.build_sql("imatches", "^test.*", path_sql, field_type=str)
        assert sql is not None
        assert "~*" in sql.as_string(None)

        sql = strategy.build_sql("not_matches", "^test.*", path_sql, field_type=str)
        assert sql is not None
        assert "!~" in sql.as_string(None)

    def test_in_operators(self, strategy, path_sql):
        """Test IN operators."""
        sql = strategy.build_sql("in", ["a", "b", "c"], path_sql, field_type=str)
        assert sql is not None
        assert "IN" in sql.as_string(None)

        sql = strategy.build_sql("nin", ["a", "b"], path_sql, field_type=str)
        assert sql is not None
        assert "NOT IN" in sql.as_string(None)

    def test_isnull_operator(self, strategy, path_sql):
        """Test NULL checking."""
        sql = strategy.build_sql("isnull", True, path_sql, field_type=str)
        assert sql is not None
        assert "IS NULL" in sql.as_string(None)

        sql = strategy.build_sql("isnull", False, path_sql, field_type=str)
        assert sql is not None
        assert "IS NOT NULL" in sql.as_string(None)

    def test_unsupported_operator_returns_none(self, strategy, path_sql):
        """Test that unsupported operators return None."""
        sql = strategy.build_sql("unknown_op", "value", path_sql, field_type=str)
        assert sql is None
