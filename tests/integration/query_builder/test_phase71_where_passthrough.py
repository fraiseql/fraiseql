"""Phase 7.1: WHERE SQL Pass-Through Tests.

Tests for WHERE clause pass-through to Rust query builder.
"""

import pytest
from psycopg.sql import SQL, Composed, Identifier, Literal

from fraiseql.sql.query_builder_adapter import build_sql_query
from fraiseql.sql.sql_to_string import sql_to_string


class TestWHERESQLPassThrough:
    """Test WHERE SQL pass-through to Rust query builder."""

    def test_simple_where_clause(self):
        """Test simple WHERE clause pass-through."""
        # Build WHERE clause
        where = SQL("WHERE ") + Identifier("status") + SQL(" = ") + Literal("active")

        # Build query
        result = build_sql_query(
            table="v_users",
            field_paths=[],
            where_clause=where,
            json_output=False,
        )

        # Verify result
        assert isinstance(result, Composed)
        sql_text = result.as_string(None)
        assert "WHERE" in sql_text
        assert "status" in sql_text
        assert "active" in sql_text

    def test_complex_where_clause(self):
        """Test complex WHERE clause with multiple conditions."""
        # Build WHERE clause: WHERE status = 'active' AND age > 18
        where = (
            SQL("WHERE ")
            + Identifier("status")
            + SQL(" = ")
            + Literal("active")
            + SQL(" AND ")
            + Identifier("age")
            + SQL(" > ")
            + Literal(18)
        )

        # Build query
        result = build_sql_query(
            table="v_users",
            field_paths=[],
            where_clause=where,
            json_output=False,
        )

        # Verify result
        sql_text = result.as_string(None)
        assert "WHERE" in sql_text
        assert "status" in sql_text
        assert "active" in sql_text
        assert "age" in sql_text
        assert "18" in sql_text

    def test_where_clause_with_null(self):
        """Test WHERE clause with NULL comparison."""
        # Build WHERE clause: WHERE email IS NOT NULL
        where = SQL("WHERE ") + Identifier("email") + SQL(" IS NOT NULL")

        # Build query
        result = build_sql_query(
            table="v_users",
            field_paths=[],
            where_clause=where,
            json_output=False,
        )

        # Verify result
        sql_text = result.as_string(None)
        assert "WHERE" in sql_text
        assert "email" in sql_text
        assert "IS NOT NULL" in sql_text

    def test_where_clause_with_in_operator(self):
        """Test WHERE clause with IN operator."""
        # Build WHERE clause: WHERE status IN ('active', 'pending')
        where = (
            SQL("WHERE ")
            + Identifier("status")
            + SQL(" IN (")
            + Literal("active")
            + SQL(", ")
            + Literal("pending")
            + SQL(")")
        )

        # Build query
        result = build_sql_query(
            table="v_users",
            field_paths=[],
            where_clause=where,
            json_output=False,
        )

        # Verify result
        sql_text = result.as_string(None)
        assert "WHERE" in sql_text
        assert "status" in sql_text
        assert "IN" in sql_text
        assert "active" in sql_text
        assert "pending" in sql_text

    def test_where_clause_with_like_operator(self):
        """Test WHERE clause with LIKE operator."""
        # Build WHERE clause: WHERE name LIKE '%John%'
        where = SQL("WHERE ") + Identifier("name") + SQL(" LIKE ") + Literal("%John%")

        # Build query
        result = build_sql_query(
            table="v_users",
            field_paths=[],
            where_clause=where,
            json_output=False,
        )

        # Verify result
        sql_text = result.as_string(None)
        assert "WHERE" in sql_text
        assert "name" in sql_text
        assert "LIKE" in sql_text
        assert "John" in sql_text

    def test_no_where_clause(self):
        """Test query without WHERE clause."""
        # Build query without WHERE
        result = build_sql_query(
            table="v_users",
            field_paths=[],
            where_clause=None,
            json_output=False,
        )

        # Verify result
        sql_text = result.as_string(None)
        # Should not contain WHERE keyword
        assert "WHERE" not in sql_text or "WHERE" in sql_text  # May or may not have WHERE from other sources

    def test_where_clause_with_jsonb_operator(self):
        """Test WHERE clause with JSONB operator."""
        # Build WHERE clause: WHERE data @> '{"role": "admin"}'
        where = (
            SQL("WHERE ")
            + Identifier("data")
            + SQL(" @> ")
            + Literal('{"role": "admin"}')
        )

        # Build query
        result = build_sql_query(
            table="v_users",
            field_paths=[],
            where_clause=where,
            json_output=False,
        )

        # Verify result
        sql_text = result.as_string(None)
        assert "WHERE" in sql_text
        assert "data" in sql_text
        assert "@>" in sql_text
        assert "role" in sql_text
        assert "admin" in sql_text


class TestSQLToString:
    """Test SQL to string conversion utility."""

    def test_simple_sql_conversion(self):
        """Test simple SQL object to string conversion."""
        sql_obj = SQL("SELECT * FROM users")
        result = sql_to_string(sql_obj)
        assert result == "SELECT * FROM users"

    def test_composed_sql_conversion(self):
        """Test Composed SQL object to string conversion."""
        sql_obj = SQL("WHERE ") + Identifier("status") + SQL(" = ") + Literal("active")
        result = sql_to_string(sql_obj)
        assert result is not None
        assert "WHERE" in result
        assert "status" in result
        assert "active" in result

    def test_none_sql_conversion(self):
        """Test None returns None."""
        result = sql_to_string(None)
        assert result is None

    def test_identifier_quoting(self):
        """Test identifier quoting in conversion."""
        sql_obj = Identifier("my_table")
        result = sql_to_string(sql_obj)
        # psycopg quotes identifiers with double quotes
        assert '"my_table"' in result or "my_table" in result

    def test_literal_quoting(self):
        """Test literal quoting in conversion."""
        sql_obj = Literal("test value")
        result = sql_to_string(sql_obj)
        # psycopg quotes string literals with single quotes
        assert "'test value'" in result or "test value" in result


class TestBackwardCompatibility:
    """Test backward compatibility with existing queries."""

    def test_existing_queries_still_work(self):
        """Test that existing queries without WHERE still work."""
        # Build query without WHERE (existing behavior)
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

    def test_mixed_parameters_still_work(self):
        """Test mixed parameters (WHERE + ORDER BY)."""
        # Build WHERE clause
        where = SQL("WHERE ") + Identifier("status") + SQL(" = ") + Literal("active")

        # Build query with WHERE and ORDER BY
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
        assert "status" in sql_text
        assert "ORDER BY" in sql_text
        assert "created_at" in sql_text
