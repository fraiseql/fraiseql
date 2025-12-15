"""Comprehensive tests for list operator SQL building."""

import pytest
from psycopg.sql import SQL

from fraiseql.sql.where.operators.lists import build_in_sql, build_notin_sql


class TestListInOperator:
    """Test IN operator with various data types."""

    def test_in_with_strings(self):
        """Test IN operator with string values."""
        path_sql = SQL("status")
        values = ["active", "pending", "approved"]
        result = build_in_sql(path_sql, values)
        result_str = str(result)
        assert " IN (" in result_str
        assert "active" in result_str
        assert "pending" in result_str
        assert "approved" in result_str
        assert result_str.endswith(")")

    def test_in_with_integers(self):
        """Test IN operator with integer values."""
        path_sql = SQL("user_id")
        values = [1, 2, 3, 5, 8]
        result = build_in_sql(path_sql, values)
        result_str = str(result)
        assert "::numeric" in result_str
        assert " IN (" in result_str
        assert "1" in result_str
        assert "2" in result_str
        assert "3" in result_str

    def test_in_with_floats(self):
        """Test IN operator with float values."""
        path_sql = SQL("score")
        values = [85.5, 92.0, 78.3]
        result = build_in_sql(path_sql, values)
        result_str = str(result)
        assert "::numeric" in result_str
        assert " IN (" in result_str
        assert "85.5" in result_str
        assert "92.0" in result_str
        assert "78.3" in result_str

    def test_in_with_mixed_types(self):
        """Test IN operator with mixed types (uses first non-None for casting)."""
        path_sql = SQL("value")
        values = [1, "text", 3.14]  # First is int, so should cast to numeric
        result = build_in_sql(path_sql, values)
        result_str = str(result)
        assert "::numeric" in result_str
        assert "1" in result_str
        assert "text" in result_str
        assert "3.14" in result_str

    def test_in_with_single_value(self):
        """Test IN operator with single value."""
        path_sql = SQL("category")
        values = ["electronics"]
        result = build_in_sql(path_sql, values)
        result_str = str(result)
        assert " IN (" in result_str
        assert "electronics" in result_str
        assert result_str.endswith(")")

    def test_in_with_empty_list(self):
        """Test IN operator with empty list."""
        path_sql = SQL("tags")
        values = []
        result = build_in_sql(path_sql, values)
        sql_str = result.as_string(None)
        assert "IN ()" in sql_str

    def test_in_with_none_values(self):
        """Test IN operator with None values."""
        path_sql = SQL("optional_field")
        values = [None, None, "value"]
        result = build_in_sql(path_sql, values)
        result_str = str(result)
        assert " IN (" in result_str
        assert "value" in result_str
        # Should not have ::numeric since first non-None is string


class TestListNotInOperator:
    """Test NOT IN operator with various data types."""

    def test_notin_with_strings(self):
        """Test NOT IN operator with string values."""
        path_sql = SQL("role")
        values = ["admin", "superuser"]
        result = build_notin_sql(path_sql, values)
        result_str = str(result)
        assert " NOT IN (" in result_str
        assert "admin" in result_str
        assert "superuser" in result_str
        assert result_str.endswith(")")

    def test_notin_with_integers(self):
        """Test NOT IN operator with integer values."""
        path_sql = SQL("excluded_ids")
        values = [100, 200, 300]
        result = build_notin_sql(path_sql, values)
        result_str = str(result)
        assert "::numeric" in result_str
        assert " NOT IN (" in result_str
        assert "100" in result_str
        assert "200" in result_str
        assert "300" in result_str

    def test_notin_with_booleans(self):
        """Test NOT IN operator with boolean values."""
        path_sql = SQL("is_active")
        values = [True, False]
        result = build_notin_sql(path_sql, values)
        result_str = str(result)
        assert " NOT IN (" in result_str
        assert "True" in result_str or "true" in result_str
        assert "False" in result_str or "false" in result_str
        # Booleans should not get ::numeric casting


class TestListOperatorErrors:
    """Test error handling for list operators."""

    def test_in_requires_list(self):
        """Test that IN operator requires a list."""
        path_sql = SQL("field")
        with pytest.raises(TypeError, match="'in' operator requires a list"):
            build_in_sql(path_sql, "not-a-list")  # type: ignore

    def test_notin_requires_list(self):
        """Test that NOT IN operator requires a list."""
        path_sql = SQL("field")
        with pytest.raises(TypeError, match="'notin' operator requires a list"):
            build_notin_sql(path_sql, 42)  # type: ignore

    def test_in_with_none_list(self):
        """Test IN operator with None instead of list."""
        path_sql = SQL("field")
        with pytest.raises(TypeError, match="'in' operator requires a list"):
            build_in_sql(path_sql, None)  # type: ignore


class TestListOperatorEdgeCases:
    """Test edge cases for list operators."""

    def test_in_with_special_characters(self):
        """Test IN operator with special characters in strings."""
        path_sql = SQL("name")
        values = ["O'Connor", "Smith-Jones", "user@domain.com"]
        result = build_in_sql(path_sql, values)
        result_str = str(result)
        assert "O'Connor" in result_str
        assert "Smith-Jones" in result_str
        assert "user@domain.com" in result_str

    def test_notin_with_large_numbers(self):
        """Test NOT IN operator with large numbers."""
        path_sql = SQL("big_id")
        values = [9223372036854775807, 9223372036854775806]  # Max int64 values
        result = build_notin_sql(path_sql, values)
        result_str = str(result)
        assert "::numeric" in result_str
        assert "9223372036854775807" in result_str
        assert "9223372036854775806" in result_str

    def test_in_with_unicode_strings(self):
        """Test IN operator with unicode strings."""
        path_sql = SQL("title")
        values = ["café", "naïve", "北京"]
        result = build_in_sql(path_sql, values)
        result_str = str(result)
        assert "café" in result_str
        assert "naïve" in result_str
        assert "北京" in result_str
