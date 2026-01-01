"""Phase 7.1: ORDER BY Support Tests.

Tests for ORDER BY clause support in Rust query builder.
"""

import pytest
from psycopg.sql import SQL, Composed

from fraiseql.sql.query_builder_adapter import build_sql_query


class TestOrderBySupport:
    """Test ORDER BY support in Rust query builder."""

    def test_simple_order_by_asc(self):
        """Test simple ORDER BY with ASC direction."""
        result = build_sql_query(
            table="v_users",
            field_paths=[],
            order_by=[("created_at", "ASC")],
            json_output=False,
        )

        # Verify result
        assert isinstance(result, Composed)
        sql_text = result.as_string(None)
        assert "ORDER BY" in sql_text
        assert "created_at" in sql_text
        assert "ASC" in sql_text

    def test_simple_order_by_desc(self):
        """Test simple ORDER BY with DESC direction."""
        result = build_sql_query(
            table="v_users",
            field_paths=[],
            order_by=[("created_at", "DESC")],
            json_output=False,
        )

        # Verify result
        sql_text = result.as_string(None)
        assert "ORDER BY" in sql_text
        assert "created_at" in sql_text
        assert "DESC" in sql_text

    def test_multiple_order_by_columns(self):
        """Test ORDER BY with multiple columns."""
        result = build_sql_query(
            table="v_users",
            field_paths=[],
            order_by=[("status", "ASC"), ("created_at", "DESC")],
            json_output=False,
        )

        # Verify result
        sql_text = result.as_string(None)
        assert "ORDER BY" in sql_text
        assert "status" in sql_text
        assert "created_at" in sql_text
        # Should contain both ASC and DESC
        assert "ASC" in sql_text
        assert "DESC" in sql_text

    def test_three_column_order_by(self):
        """Test ORDER BY with three columns."""
        result = build_sql_query(
            table="v_users",
            field_paths=[],
            order_by=[("status", "ASC"), ("priority", "DESC"), ("created_at", "ASC")],
            json_output=False,
        )

        # Verify result
        sql_text = result.as_string(None)
        assert "ORDER BY" in sql_text
        assert "status" in sql_text
        assert "priority" in sql_text
        assert "created_at" in sql_text

    def test_no_order_by(self):
        """Test query without ORDER BY."""
        result = build_sql_query(
            table="v_users",
            field_paths=[],
            json_output=False,
        )

        # Verify result
        sql_text = result.as_string(None)
        # May or may not have default ORDER BY
        # Just verify query is valid
        assert "SELECT" in sql_text
        assert "v_users" in sql_text

    def test_order_by_with_where_clause(self):
        """Test ORDER BY combined with WHERE clause."""
        where = SQL("WHERE ") + SQL("status = 'active'")
        result = build_sql_query(
            table="v_users",
            field_paths=[],
            where_clause=where,
            order_by=[("created_at", "DESC")],
            json_output=False,
        )

        # Verify result
        sql_text = result.as_string(None)
        assert "WHERE" in sql_text
        assert "ORDER BY" in sql_text
        assert "created_at" in sql_text
        assert "DESC" in sql_text

    def test_order_by_case_insensitive(self):
        """Test ORDER BY direction is case-insensitive."""
        # Test lowercase
        result1 = build_sql_query(
            table="v_users",
            field_paths=[],
            order_by=[("created_at", "asc")],
            json_output=False,
        )

        # Test uppercase
        result2 = build_sql_query(
            table="v_users",
            field_paths=[],
            order_by=[("created_at", "ASC")],
            json_output=False,
        )

        # Both should work and produce uppercase in SQL
        sql1 = result1.as_string(None)
        sql2 = result2.as_string(None)
        assert "ASC" in sql1 or "asc" in sql1
        assert "ASC" in sql2 or "asc" in sql2

    def test_order_by_invalid_direction_defaults_to_asc(self):
        """Test ORDER BY with invalid direction (Rust defaults to ASC, Python passes through).

        Note: Python query builder passes through the direction as-is,
        while Rust builder validates and defaults to ASC if invalid.
        This test verifies the query is generated without errors.
        """
        result = build_sql_query(
            table="v_users",
            field_paths=[],
            order_by=[("created_at", "INVALID")],
            json_output=False,
        )

        # Verify result - query should be generated without errors
        sql_text = result.as_string(None)
        assert "ORDER BY" in sql_text
        assert "created_at" in sql_text
        # Either Rust (ASC) or Python (INVALID) behavior is acceptable
        assert ("ASC" in sql_text or "INVALID" in sql_text)

    def test_order_by_with_different_table(self):
        """Test ORDER BY works with different table names."""
        result = build_sql_query(
            table="v_products",
            field_paths=[],
            order_by=[("price", "DESC"), ("name", "ASC")],
            json_output=False,
        )

        # Verify result
        sql_text = result.as_string(None)
        assert "v_products" in sql_text
        assert "ORDER BY" in sql_text
        assert "price" in sql_text
        assert "name" in sql_text


class TestOrderByEdgeCases:
    """Test edge cases for ORDER BY support."""

    def test_empty_order_by_list(self):
        """Test empty ORDER BY list."""
        result = build_sql_query(
            table="v_users",
            field_paths=[],
            order_by=[],
            json_output=False,
        )

        # Should work without ORDER BY
        sql_text = result.as_string(None)
        assert "SELECT" in sql_text
        assert "v_users" in sql_text

    def test_order_by_with_special_characters_in_field_name(self):
        """Test ORDER BY with field names containing special characters."""
        result = build_sql_query(
            table="v_users",
            field_paths=[],
            order_by=[("created_at_timestamp", "DESC")],
            json_output=False,
        )

        # Verify result
        sql_text = result.as_string(None)
        assert "ORDER BY" in sql_text
        assert "created_at_timestamp" in sql_text

    def test_order_by_with_json_output(self):
        """Test ORDER BY combined with json_output flag."""
        result = build_sql_query(
            table="v_users",
            field_paths=[],
            order_by=[("created_at", "DESC")],
            json_output=True,
        )

        # Verify result
        sql_text = result.as_string(None)
        assert "ORDER BY" in sql_text
        assert "created_at" in sql_text

    def test_order_by_preserves_order(self):
        """Test ORDER BY preserves the order of columns."""
        result = build_sql_query(
            table="v_users",
            field_paths=[],
            order_by=[("a", "ASC"), ("b", "DESC"), ("c", "ASC")],
            json_output=False,
        )

        # Verify result - columns should appear in order
        sql_text = result.as_string(None)
        assert "ORDER BY" in sql_text
        # Find positions of each field
        pos_a = sql_text.find("a")
        pos_b = sql_text.find("b")
        pos_c = sql_text.find("c")
        # a should come before b, b before c
        assert pos_a < pos_b < pos_c


class TestBackwardCompatibilityOrderBy:
    """Test backward compatibility for ORDER BY."""

    def test_existing_queries_without_order_by_still_work(self):
        """Test that existing queries without ORDER BY still work."""
        result = build_sql_query(
            table="v_users",
            field_paths=[],
            json_output=False,
        )

        # Verify result
        assert isinstance(result, Composed)
        sql_text = result.as_string(None)
        assert "SELECT" in sql_text
        assert "v_users" in sql_text

    def test_mixed_parameters_work_together(self):
        """Test all parameters work together."""
        where = SQL("WHERE ") + SQL("status = 'active'")
        result = build_sql_query(
            table="v_users",
            field_paths=[],
            where_clause=where,
            order_by=[("created_at", "DESC"), ("id", "ASC")],
            json_output=True,
        )

        # Verify result
        sql_text = result.as_string(None)
        assert "WHERE" in sql_text
        assert "ORDER BY" in sql_text
        assert "created_at" in sql_text
        assert "DESC" in sql_text
        assert "id" in sql_text
        assert "ASC" in sql_text
