"""Comprehensive tests for date range operator SQL building."""

import pytest
from psycopg.sql import SQL

from fraiseql.sql.where.operators.date_range import (
    build_adjacent_sql,
    build_contains_date_sql,
    build_daterange_eq_sql,
    build_daterange_in_sql,
    build_daterange_neq_sql,
    build_daterange_notin_sql,
    build_not_left_sql,
    build_not_right_sql,
    build_overlaps_sql,
    build_strictly_left_sql,
    build_strictly_right_sql,
)


class TestDateRangeBasicOperators:
    """Test basic date range comparison."""

    def test_eq_date_range(self):
        """Test date range equality."""
        path_sql = SQL("period")
        result = build_daterange_eq_sql(path_sql, "[2024-01-01,2024-01-31]")
        result_str = str(result)
        assert "=" in result_str
        assert "[2024-01-01,2024-01-31]" in result_str

    def test_neq_date_range(self):
        """Test date range inequality."""
        path_sql = SQL("period")
        result = build_daterange_neq_sql(path_sql, "[2024-01-01,2024-01-31]")
        result_str = str(result)
        assert "!=" in result_str

    def test_in_date_ranges(self):
        """Test date range IN list."""
        path_sql = SQL("period")
        result = build_daterange_in_sql(
            path_sql, ["[2024-01-01,2024-01-31]", "[2024-02-01,2024-02-28]"]
        )
        result_str = str(result)
        assert "IN" in result_str

    def test_notin_date_ranges(self):
        """Test date range NOT IN list."""
        path_sql = SQL("period")
        result = build_daterange_notin_sql(path_sql, ["[2024-01-01,2024-01-31]"])
        result_str = str(result)
        assert "NOT IN" in result_str


class TestDateRangeOverlaps:
    """Test overlaps operator."""

    def test_overlaps_basic(self):
        """Test if two ranges overlap."""
        path_sql = SQL("period")
        result = build_overlaps_sql(path_sql, "[2024-01-15,2024-02-15]")
        result_str = str(result)
        assert "&&" in result_str  # PostgreSQL overlap operator
        assert "[2024-01-15,2024-02-15]" in result_str

    def test_overlaps_partial(self):
        """Test partial overlap."""
        path_sql = SQL("period")
        result = build_overlaps_sql(path_sql, "[2024-01-15,2024-02-15]")
        result_str = str(result)
        assert "&&" in result_str


class TestDateRangeContains:
    """Test containment operators."""

    def test_contains_date(self):
        """Test if range contains a specific date."""
        path_sql = SQL("period")
        result = build_contains_date_sql(path_sql, "2024-06-15")
        result_str = str(result)
        assert "@>" in result_str  # PostgreSQL contains operator
        assert "2024-06-15" in result_str


class TestDateRangeAdjacency:
    """Test adjacent ranges."""

    def test_adjacent_ranges(self):
        """Test if ranges are adjacent."""
        path_sql = SQL("period")
        result = build_adjacent_sql(path_sql, "[2024-02-01,2024-02-28]")
        result_str = str(result)
        assert "-|-" in result_str  # PostgreSQL adjacent operator


class TestDateRangeOrdering:
    """Test range ordering operators."""

    def test_strictly_left_range(self):
        """Test if range is entirely left of another."""
        path_sql = SQL("period")
        result = build_strictly_left_sql(path_sql, "[2024-02-01,2024-02-28]")
        result_str = str(result)
        assert "<<" in result_str  # PostgreSQL strictly left operator

    def test_strictly_right_range(self):
        """Test if range is entirely right of another."""
        path_sql = SQL("period")
        result = build_strictly_right_sql(path_sql, "[2023-12-01,2023-12-31]")
        result_str = str(result)
        assert ">>" in result_str  # PostgreSQL strictly right operator

    def test_not_left_range(self):
        """Test not left of range."""
        path_sql = SQL("period")
        result = build_not_left_sql(path_sql, "[2024-01-01,2024-01-31]")
        result_str = str(result)
        assert "&>" in result_str  # PostgreSQL not left of operator

    def test_not_right_range(self):
        """Test not right of range."""
        path_sql = SQL("period")
        result = build_not_right_sql(path_sql, "[2024-01-01,2024-01-31]")
        result_str = str(result)
        assert "&<" in result_str  # PostgreSQL not right of operator


class TestDateRangeBoundaries:
    """Test boundary conditions."""

    def test_inclusive_bounds(self):
        """Test inclusive [) bounds."""
        path_sql = SQL("period")
        result = build_daterange_eq_sql(path_sql, "[2024-01-01,2024-01-31)")
        result_str = str(result)
        assert "[2024-01-01,2024-01-31)" in result_str

    def test_exclusive_bounds(self):
        """Test exclusive () bounds."""
        path_sql = SQL("period")
        result = build_daterange_eq_sql(path_sql, "(2024-01-01,2024-01-31)")
        result_str = str(result)
        assert "(2024-01-01,2024-01-31)" in result_str


class TestDateRangeEdgeCases:
    """Test edge cases."""

    def test_single_day_range(self):
        """Test range with single day."""
        path_sql = SQL("period")
        result = build_daterange_eq_sql(path_sql, "[2024-01-01,2024-01-01]")
        result_str = str(result)
        assert "[2024-01-01,2024-01-01]" in result_str

    def test_month_boundary(self):
        """Test month boundary ranges."""
        path_sql = SQL("period")
        result = build_overlaps_sql(path_sql, "[2024-01-31,2024-02-01]")
        result_str = str(result)
        assert "[2024-01-31,2024-02-01]" in result_str

    def test_year_boundary(self):
        """Test year boundary ranges."""
        path_sql = SQL("period")
        result = build_overlaps_sql(path_sql, "[2023-12-31,2024-01-01]")
        result_str = str(result)
        assert "[2023-12-31,2024-01-01]" in result_str
