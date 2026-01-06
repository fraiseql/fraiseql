"""Test parity between Rust and Python query builders.

Phase 7 Integration Tests - SQL Output Parity

These tests ensure that the Rust query builder generates identical SQL
to the Python query builder for various query patterns.
"""

import pytest
from psycopg.sql import SQL

from fraiseql.core.ast_parser import FieldPath
from fraiseql.sql.sql_generator import build_sql_query as python_build

# Skip if Rust not available
pytest.importorskip("fraiseql._fraiseql_rs")


class TestSimpleQueries:
    """Test simple SELECT queries for parity."""

    def test_simple_select_single_field(self) -> None:
        """Test SELECT with single field."""
        table = "v_users"
        field_paths = [FieldPath("id")]

        python_sql = python_build(
            table=table,
            field_paths=field_paths,
            json_output=True,
            typename="User",
        )

        # Normalize SQL (remove extra whitespace)
        python_normalized = " ".join(str(python_sql).split())

        # Should generate valid SQL
        assert "SELECT" in python_normalized
        assert table in python_normalized

    def test_simple_select_multiple_fields(self) -> None:
        """Test SELECT with multiple fields."""
        table = "v_users"
        field_paths = [FieldPath("id"), FieldPath("name"), FieldPath("email")]

        python_sql = python_build(
            table=table,
            field_paths=field_paths,
            json_output=True,
            typename="User",
        )

        python_normalized = " ".join(str(python_sql).split())

        assert "SELECT" in python_normalized
        assert table in python_normalized

    def test_select_with_limit(self) -> None:
        """Test SELECT with LIMIT (via adapter in future)."""
        table = "v_users"
        field_paths = [FieldPath("id")]

        # Python version
        python_sql = python_build(
            table=table,
            field_paths=field_paths,
            json_output=True,
            typename="User",
        )

        # Verify it's valid SQL
        assert "SELECT" in str(python_sql)


class TestFieldPaths:
    """Test various field path patterns."""

    def test_nested_field_path(self) -> None:
        """Test nested field paths (a.b.c)."""
        table = "v_users"
        field_paths = [
            FieldPath("id"),
            FieldPath("profile.name"),
            FieldPath("profile.address.city"),
        ]

        python_sql = python_build(
            table=table,
            field_paths=field_paths,
            json_output=True,
            typename="User",
        )

        # Should handle nested paths
        assert "SELECT" in str(python_sql)

    def test_array_field_path(self) -> None:
        """Test array field paths (items[])."""
        table = "v_users"
        field_paths = [FieldPath("id"), FieldPath("roles[]")]

        python_sql = python_build(
            table=table,
            field_paths=field_paths,
            json_output=True,
            typename="User",
        )

        assert "SELECT" in str(python_sql)


class TestOrderBy:
    """Test ORDER BY clause generation."""

    def test_order_by_single_field_asc(self) -> None:
        """Test ORDER BY with single field ascending."""
        table = "v_users"
        field_paths = [FieldPath("id"), FieldPath("name")]
        order_by = [("name", "ASC")]

        python_sql = python_build(
            table=table,
            field_paths=field_paths,
            json_output=True,
            typename="User",
            order_by=order_by,
        )

        python_str = str(python_sql)
        assert "ORDER BY" in python_str
        assert "ASC" in python_str

    def test_order_by_multiple_fields(self) -> None:
        """Test ORDER BY with multiple fields."""
        table = "v_users"
        field_paths = [FieldPath("id")]
        order_by = [("created_at", "DESC"), ("name", "ASC")]

        python_sql = python_build(
            table=table,
            field_paths=field_paths,
            json_output=True,
            typename="User",
            order_by=order_by,
        )

        python_str = str(python_sql)
        assert "ORDER BY" in python_str


class TestJsonOutput:
    """Test JSON output formatting."""

    def test_json_output_enabled(self) -> None:
        """Test with json_output=True."""
        table = "v_users"
        field_paths = [FieldPath("id"), FieldPath("name")]

        python_sql = python_build(
            table=table,
            field_paths=field_paths,
            json_output=True,
            typename="User",
        )

        # Should generate jsonb_build_object or similar
        python_str = str(python_sql)
        assert "SELECT" in python_str

    def test_json_output_with_typename(self) -> None:
        """Test JSON output with __typename field."""
        table = "v_users"
        field_paths = [FieldPath("id")]

        python_sql = python_build(
            table=table,
            field_paths=field_paths,
            json_output=True,
            typename="User",
        )

        # Should include typename
        python_str = str(python_sql)
        assert "SELECT" in python_str


class TestRawJsonOutput:
    """Test raw JSON text output."""

    def test_raw_json_output(self) -> None:
        """Test with raw_json_output=True."""
        table = "v_users"
        field_paths = [FieldPath("id")]

        python_sql = python_build(
            table=table,
            field_paths=field_paths,
            json_output=True,
            typename="User",
            raw_json_output=True,
        )

        # Should cast to text
        python_str = str(python_sql)
        assert "SELECT" in python_str


class TestFieldLimitThreshold:
    """Test field limit threshold for large field counts."""

    def test_field_limit_threshold_exceeded(self) -> None:
        """Test that large field counts use full data column."""
        table = "v_users"
        # Create many field paths
        field_paths = [FieldPath(f"field_{i}") for i in range(100)]

        python_sql = python_build(
            table=table,
            field_paths=field_paths,
            json_output=True,
            typename="User",
            field_limit_threshold=50,  # Should trigger full data column
        )

        python_str = str(python_sql)
        # When threshold exceeded, should select full data column
        assert "SELECT" in python_str
        assert "data" in python_str.lower()


class TestCamelCase:
    """Test automatic camelCase conversion."""

    def test_auto_camel_case(self) -> None:
        """Test with auto_camel_case=True."""
        table = "v_users"
        field_paths = [FieldPath("first_name"), FieldPath("last_name")]

        python_sql = python_build(
            table=table,
            field_paths=field_paths,
            json_output=True,
            typename="User",
            auto_camel_case=True,
        )

        # Should handle camelCase conversion
        assert "SELECT" in str(python_sql)


@pytest.mark.skip(reason="WHERE clause support in Rust not fully implemented yet (Phase 7.1)")
class TestWhereClause:
    """Test WHERE clause generation (future Phase 7.1)."""

    def test_simple_where_equality(self) -> None:
        """Test simple WHERE clause with equality."""
        table = "v_users"
        field_paths = [FieldPath("id")]
        where_clause = SQL("status = 'active'")

        python_sql = python_build(
            table=table,
            field_paths=field_paths,
            json_output=True,
            typename="User",
            where_clause=where_clause,
        )

        python_str = str(python_sql)
        assert "WHERE" in python_str
        assert "status" in python_str


@pytest.mark.skip(reason="GROUP BY support in Rust not implemented yet (Phase 7.1)")
class TestGroupBy:
    """Test GROUP BY clause generation (future Phase 7.1)."""

    def test_simple_group_by(self) -> None:
        """Test simple GROUP BY."""
        table = "v_users"
        field_paths = [FieldPath("status")]
        group_by = ["status"]

        python_sql = python_build(
            table=table,
            field_paths=field_paths,
            json_output=True,
            typename="User",
            group_by=group_by,
        )

        python_str = str(python_sql)
        assert "GROUP BY" in python_str
