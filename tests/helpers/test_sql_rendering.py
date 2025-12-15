"""Unit tests for SQL rendering utilities.

Tests the render_sql_for_testing() function and related utilities
that convert psycopg.sql objects to human-readable SQL strings.
"""

import pytest
from psycopg.sql import SQL, Identifier, Literal, Placeholder

from tests.helpers.sql_rendering import (
    assert_sql_contains,
    assert_sql_pattern,
    render_sql_for_testing,
    render_sql_list,
)


class TestRenderSQLForTesting:
    """Test the core render_sql_for_testing() function."""

    def test_renders_simple_sql_object(self) -> None:
        """Test rendering a simple SQL object."""
        sql = SQL("SELECT * FROM users")
        result = render_sql_for_testing(sql)

        assert result == "SELECT * FROM users"
        assert isinstance(result, str)

    def test_renders_composed_sql(self) -> None:
        """Test rendering composed SQL objects."""
        field = SQL("(data ->> 'port')")
        cast = SQL("::numeric")
        operator = SQL(" = ")
        value = Literal(443)

        composed = field + cast + operator + value
        result = render_sql_for_testing(composed)

        assert result == "(data ->> 'port')::numeric = 443"
        assert "::numeric" in result
        assert "443" in result

    def test_renders_literal_values(self) -> None:
        """Test rendering literal values."""
        # Numeric literal
        lit_num = Literal(42)
        assert render_sql_for_testing(lit_num) == "42"

        # String literal
        lit_str = Literal("hello")
        assert render_sql_for_testing(lit_str) == "'hello'"

        # Boolean literal (renders as PostgreSQL boolean)
        lit_bool = Literal(True)
        result_bool = render_sql_for_testing(lit_bool)
        assert "true" in result_bool.lower() or "TRUE" in result_bool

    def test_renders_identifier(self) -> None:
        """Test rendering SQL identifiers."""
        ident = Identifier("column_name")
        result = render_sql_for_testing(ident)

        # Identifiers are quoted in PostgreSQL
        assert "column_name" in result

    def test_renders_placeholder(self) -> None:
        """Test rendering placeholders."""
        placeholder = Placeholder("param1")
        result = render_sql_for_testing(placeholder)

        # Placeholders render as %(param1)s in psycopg3
        assert "param1" in result

    def test_renders_complex_composed_sql(self) -> None:
        """Test rendering complex composed SQL with multiple parts."""
        # Build: WHERE (data->>'age')::int > 18 AND (data->>'active')::boolean = true
        age_field = SQL("(data->>'age')")
        age_cast = SQL("::int")
        age_op = SQL(" > ")
        age_val = Literal(18)

        active_field = SQL("(data->>'active')")
        active_cast = SQL("::boolean")
        active_op = SQL(" = ")
        active_val = Literal(True)

        and_op = SQL(" AND ")

        composed = (
            age_field
            + age_cast
            + age_op
            + age_val
            + and_op
            + active_field
            + active_cast
            + active_op
            + active_val
        )

        result = render_sql_for_testing(composed)

        assert "(data->>'age')" in result
        assert "::int" in result
        assert "> 18" in result
        assert "AND" in result
        assert "(data->>'active')" in result
        assert "::boolean" in result

    def test_handles_none_input(self) -> None:
        """Test handling None input."""
        result = render_sql_for_testing(None)
        assert result == "NULL"

    def test_handles_string_input(self) -> None:
        """Test handling already-stringified SQL."""
        sql_str = "SELECT * FROM users WHERE id = 1"
        result = render_sql_for_testing(sql_str)
        assert result == sql_str

    def test_handles_bytes_input(self) -> None:
        """Test handling bytes input."""
        sql_bytes = b"SELECT * FROM users"
        result = render_sql_for_testing(sql_bytes)
        assert result == "SELECT * FROM users"

    def test_handles_numeric_input(self) -> None:
        """Test handling raw numeric values."""
        assert render_sql_for_testing(42) == "42"
        assert render_sql_for_testing(3.14) == "3.14"

    def test_handles_boolean_input(self) -> None:
        """Test handling raw boolean values."""
        assert render_sql_for_testing(True) == "True"
        assert render_sql_for_testing(False) == "False"


class TestRenderSQLList:
    """Test the render_sql_list() batch rendering function."""

    def test_renders_list_of_sql_objects(self) -> None:
        """Test rendering a list of SQL objects."""
        sql_objects = [
            SQL("SELECT * FROM users"),
            SQL("WHERE id = ") + Literal(1),
            SQL("ORDER BY name"),
        ]

        results = render_sql_list(sql_objects)

        assert len(results) == 3
        assert results[0] == "SELECT * FROM users"
        assert "WHERE id = 1" in results[1]
        assert results[2] == "ORDER BY name"

    def test_renders_empty_list(self) -> None:
        """Test rendering an empty list."""
        results = render_sql_list([])
        assert results == []

    def test_renders_mixed_types(self) -> None:
        """Test rendering list with mixed types."""
        mixed = [SQL("SELECT"), Literal(42), "raw string", None]

        results = render_sql_list(mixed)

        assert results[0] == "SELECT"
        assert results[1] == "42"
        assert results[2] == "raw string"
        assert results[3] == "NULL"


class TestAssertSQLContains:
    """Test the assert_sql_contains() convenience function."""

    def test_passes_when_all_substrings_present(self) -> None:
        """Test assertion passes when all substrings are present."""
        sql = SQL("(data ->> 'port')::numeric = ") + Literal(443)

        # Should not raise
        assert_sql_contains(sql, "::numeric", "(data ->> 'port')", "443")

    def test_fails_when_substring_missing(self) -> None:
        """Test assertion fails when substring is missing."""
        sql = SQL("SELECT * FROM users")

        with pytest.raises(AssertionError) as exc_info:
            assert_sql_contains(sql, "WHERE")

        assert "Expected substring 'WHERE' not found" in str(exc_info.value)
        assert "SELECT * FROM users" in str(exc_info.value)

    def test_accepts_multiple_substrings(self) -> None:
        """Test checking multiple substrings at once."""
        sql = SQL("SELECT id, name, email FROM users WHERE active = true")

        # Should not raise
        assert_sql_contains(sql, "SELECT", "FROM users", "WHERE", "active = true")

    def test_handles_empty_substrings(self) -> None:
        """Test with no substrings to check."""
        sql = SQL("SELECT * FROM users")

        # Should not raise (no assertions to check)
        assert_sql_contains(sql)


class TestAssertSQLPattern:
    """Test the assert_sql_pattern() regex matching function."""

    def test_passes_when_pattern_matches(self) -> None:
        """Test assertion passes when pattern matches."""
        sql = SQL("(data ->> 'port')::numeric = ") + Literal(443)

        # Should not raise
        assert_sql_pattern(sql, r"\(data ->> 'port'\)::numeric = \d+")

    def test_fails_when_pattern_not_found(self) -> None:
        """Test assertion fails when pattern doesn't match."""
        sql = SQL("SELECT * FROM users")

        with pytest.raises(AssertionError) as exc_info:
            assert_sql_pattern(sql, r"WHERE id = \d+")

        assert "Pattern" in str(exc_info.value)
        assert "not found" in str(exc_info.value)

    def test_supports_complex_regex_patterns(self) -> None:
        """Test with complex regex patterns."""
        sql = SQL("WHERE age BETWEEN ") + Literal(18) + SQL(" AND ") + Literal(65)

        # Should not raise
        assert_sql_pattern(sql, r"WHERE age BETWEEN \d+ AND \d+")

    def test_pattern_with_special_characters(self) -> None:
        """Test pattern matching with SQL special characters."""
        sql = SQL("(data->>'name')::text = 'John''s'")

        # Should not raise - matches single quotes and ->
        assert_sql_pattern(sql, r"\(data->>'name'\)::text")


class TestRealWorldScenarios:
    """Test scenarios from actual failing tests."""

    def test_numeric_casting_structure(self) -> None:
        """Test the exact scenario from test_sql_structure_validation.py."""
        # This is what the failing test was doing
        jsonb_path = SQL("(data ->> 'port')")
        operator = SQL(" = ")
        value = Literal(443)
        cast = SQL("::numeric")

        # Build SQL the way operator strategies do
        result = jsonb_path + cast + operator + value

        # This was failing because str(result) returned 'None'
        sql_str = render_sql_for_testing(result)

        # Now these assertions work
        assert "::numeric" in sql_str
        assert "(data ->> 'port')" in sql_str
        assert "443" in sql_str
        assert "=" in sql_str

    def test_boolean_text_comparison(self) -> None:
        """Test boolean comparison scenario."""
        jsonb_path = SQL("(data ->> 'is_active')")
        cast = SQL("::boolean")
        operator = SQL(" = ")
        value = Literal(True)

        result = jsonb_path + cast + operator + value
        sql_str = render_sql_for_testing(result)

        assert "(data ->> 'is_active')" in sql_str
        assert "::boolean" in sql_str

    def test_list_operation_structure(self) -> None:
        """Test IN list operation scenario."""
        field = SQL("(data ->> 'status')")
        operator = SQL(" IN (")
        values = SQL(", ").join([Literal("active"), Literal("pending"), Literal("approved")])
        closing = SQL(")")

        result = field + operator + values + closing
        sql_str = render_sql_for_testing(result)

        assert "(data ->> 'status')" in sql_str
        assert "IN" in sql_str
        assert "active" in sql_str
        assert "pending" in sql_str
        assert "approved" in sql_str

    def test_composed_where_clause(self) -> None:
        """Test a complete WHERE clause composition."""
        # WHERE (data->>'age')::int > 18 AND (data->>'active')::boolean = true
        age_condition = SQL("(data->>'age')::int > ") + Literal(18)
        active_condition = SQL("(data->>'active')::boolean = ") + Literal(True)
        where_clause = SQL("WHERE ") + age_condition + SQL(" AND ") + active_condition

        sql_str = render_sql_for_testing(where_clause)

        assert "WHERE" in sql_str
        assert "(data->>'age')::int" in sql_str
        assert "> 18" in sql_str
        assert "AND" in sql_str
        assert "(data->>'active')::boolean" in sql_str


class TestEdgeCases:
    """Test edge cases and error handling."""

    def test_nested_composed_objects(self) -> None:
        """Test deeply nested Composed objects."""
        inner = SQL("a") + SQL("b")
        middle = inner + SQL("c")
        outer = middle + SQL("d")

        result = render_sql_for_testing(outer)
        assert result == "abcd"

    def test_empty_sql_object(self) -> None:
        """Test rendering empty SQL object."""
        empty = SQL("")
        result = render_sql_for_testing(empty)
        assert result == ""

    def test_sql_with_unicode(self) -> None:
        """Test SQL with unicode characters."""
        sql = SQL("SELECT '") + Literal("café") + SQL("'")
        result = render_sql_for_testing(sql)

        assert "café" in result or "caf" in result  # May be escaped

    def test_very_long_sql_statement(self) -> None:
        """Test rendering very long SQL statements."""
        # Build a long SQL statement
        parts = [SQL(f"col{i}, ") for i in range(100)]
        sql = SQL("SELECT ") + SQL("").join(parts) + SQL("FROM table")

        result = render_sql_for_testing(sql)

        assert "SELECT" in result
        assert "FROM table" in result
        assert len(result) > 500  # Should be quite long
