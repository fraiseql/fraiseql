"""Comprehensive tests for date range operator SQL building.

Consolidated from test_daterange_operators_sql_building.py and test_daterange_operators_complete.py.
"""

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

    def test_build_daterange_equality_sql(self) -> None:
        """Test DateRange equality operator with proper daterange casting."""
        path_sql = SQL("data->>'period'")
        value = "[2023-01-01,2023-12-31]"

        result = build_daterange_eq_sql(path_sql, value)
        expected = "(data->>'period')::daterange = '[2023-01-01,2023-12-31]'::daterange"

        assert result.as_string(None) == expected

    def test_build_daterange_inequality_sql(self) -> None:
        """Test DateRange inequality operator with proper daterange casting."""
        path_sql = SQL("data->>'period'")
        value = "[2023-01-01,2023-12-31]"

        result = build_daterange_neq_sql(path_sql, value)
        expected = "(data->>'period')::daterange != '[2023-01-01,2023-12-31]'::daterange"

        assert result.as_string(None) == expected

    def test_build_daterange_in_list_sql(self) -> None:
        """Test DateRange IN list with multiple daterange values."""
        path_sql = SQL("data->>'period'")
        value = ["[2023-01-01,2023-12-31]", "[2024-01-01,2024-12-31]"]

        result = build_daterange_in_sql(path_sql, value)
        expected = "(data->>'period')::daterange IN ('[2023-01-01,2023-12-31]'::daterange, '[2024-01-01,2024-12-31]'::daterange)"

        assert result.as_string(None) == expected

    def test_build_daterange_not_in_list_sql(self) -> None:
        """Test DateRange NOT IN list with multiple daterange values."""
        path_sql = SQL("data->>'period'")
        value = ["[2023-01-01,2023-12-31]", "[2024-01-01,2024-12-31]"]

        result = build_daterange_notin_sql(path_sql, value)
        expected = "(data->>'period')::daterange NOT IN ('[2023-01-01,2023-12-31]'::daterange, '[2024-01-01,2024-12-31]'::daterange)"

        assert result.as_string(None) == expected

    def test_build_daterange_single_item_in_list(self) -> None:
        """Test DateRange IN list with single value."""
        path_sql = SQL("data->>'period'")
        value = ["[2023-01-01,2023-12-31]"]

        result = build_daterange_in_sql(path_sql, value)
        expected = "(data->>'period')::daterange IN ('[2023-01-01,2023-12-31]'::daterange)"

        assert result.as_string(None) == expected

    def test_build_daterange_different_bracket_types(self) -> None:
        """Test DateRange operators with different bracket types (inclusive/exclusive)."""
        path_sql = SQL("data->>'period'")

        # Test inclusive start, exclusive end
        result_inc_exc = build_daterange_eq_sql(path_sql, "[2023-01-01,2023-12-31)")
        expected_inc_exc = "(data->>'period')::daterange = '[2023-01-01,2023-12-31)'::daterange"
        assert result_inc_exc.as_string(None) == expected_inc_exc

        # Test exclusive start, inclusive end
        result_exc_inc = build_daterange_eq_sql(path_sql, "(2023-01-01,2023-12-31]")
        expected_exc_inc = "(data->>'period')::daterange = '(2023-01-01,2023-12-31]'::daterange"
        assert result_exc_inc.as_string(None) == expected_exc_inc

    def test_build_daterange_empty_list_handling(self) -> None:
        """Test DateRange operators handle empty lists gracefully."""
        path_sql = SQL("data->>'period'")
        value = []

        result_in = build_daterange_in_sql(path_sql, value)
        expected_in = "(data->>'period')::daterange IN ()"
        assert result_in.as_string(None) == expected_in

        result_notin = build_daterange_notin_sql(path_sql, value)
        expected_notin = "(data->>'period')::daterange NOT IN ()"
        assert result_notin.as_string(None) == expected_notin


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


class TestDateRangeSpecificOperators:
    """Test DateRange-specific operators for range operations."""

    def test_build_contains_date_sql(self) -> None:
        """Test DateRange contains_date (@>) operator."""
        path_sql = SQL("data->>'period'")
        value = "2023-07-15"

        result = build_contains_date_sql(path_sql, value)
        expected = "(data->>'period')::daterange @> '2023-07-15'"

        assert result.as_string(None) == expected

    def test_build_overlaps_sql(self) -> None:
        """Test DateRange overlaps (&&) operator."""
        path_sql = SQL("data->>'period'")
        value = "[2023-06-01,2023-08-31]"

        result = build_overlaps_sql(path_sql, value)
        expected = "(data->>'period')::daterange && '[2023-06-01,2023-08-31]'::daterange"

        assert result.as_string(None) == expected

    def test_build_adjacent_sql(self) -> None:
        """Test DateRange adjacent (-|-) operator."""
        path_sql = SQL("data->>'period'")
        value = "[2024-01-01,2024-12-31]"

        result = build_adjacent_sql(path_sql, value)
        expected = "(data->>'period')::daterange -|- '[2024-01-01,2024-12-31]'::daterange"

        assert result.as_string(None) == expected

    def test_build_strictly_left_sql(self) -> None:
        """Test DateRange strictly_left (<<) operator."""
        path_sql = SQL("data->>'period'")
        value = "[2024-01-01,2024-12-31]"

        result = build_strictly_left_sql(path_sql, value)
        expected = "(data->>'period')::daterange << '[2024-01-01,2024-12-31]'::daterange"

        assert result.as_string(None) == expected

    def test_build_strictly_right_sql(self) -> None:
        """Test DateRange strictly_right (>>) operator."""
        path_sql = SQL("data->>'period'")
        value = "[2022-01-01,2022-12-31]"

        result = build_strictly_right_sql(path_sql, value)
        expected = "(data->>'period')::daterange >> '[2022-01-01,2022-12-31]'::daterange"

        assert result.as_string(None) == expected

    def test_build_not_left_sql(self) -> None:
        """Test DateRange not_left (&>) operator."""
        path_sql = SQL("data->>'period'")
        value = "[2023-01-01,2023-12-31]"

        result = build_not_left_sql(path_sql, value)
        expected = "(data->>'period')::daterange &> '[2023-01-01,2023-12-31]'::daterange"

        assert result.as_string(None) == expected

    def test_build_not_right_sql(self) -> None:
        """Test DateRange not_right (&<) operator."""
        path_sql = SQL("data->>'period'")
        value = "[2023-01-01,2023-12-31]"

        result = build_not_right_sql(path_sql, value)
        expected = "(data->>'period')::daterange &< '[2023-01-01,2023-12-31]'::daterange"

        assert result.as_string(None) == expected

    def test_range_operators_complex_ranges(self) -> None:
        """Test range operators with complex date ranges."""
        path_sql = SQL("data->>'fiscal_year'")

        # Test with time components and timezone
        complex_range = "[2023-04-01 00:00:00,2024-03-31 23:59:59]"

        result_overlaps = build_overlaps_sql(path_sql, complex_range)
        expected_overlaps = "(data->>'fiscal_year')::daterange && '[2023-04-01 00:00:00,2024-03-31 23:59:59]'::daterange"
        assert result_overlaps.as_string(None) == expected_overlaps

        # Test contains with timestamp
        result_contains = build_contains_date_sql(path_sql, "2023-07-15 12:30:00")
        expected_contains = "(data->>'fiscal_year')::daterange @> '2023-07-15 12:30:00'"
        assert result_contains.as_string(None) == expected_contains


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


class TestDateRangeValidation:
    """Test DateRange operator validation and error handling."""

    def test_daterange_in_requires_list(self) -> None:
        """Test that DateRange 'in' operator requires a list."""
        path_sql = SQL("data->>'period'")

        with pytest.raises(TypeError, match="'in' operator requires a list"):
            build_daterange_in_sql(path_sql, "[2023-01-01,2023-12-31]")  # type: ignore[arg-type]

    def test_daterange_notin_requires_list(self) -> None:
        """Test that DateRange 'notin' operator requires a list."""
        path_sql = SQL("data->>'period'")

        with pytest.raises(TypeError, match="'notin' operator requires a list"):
            build_daterange_notin_sql(path_sql, "[2023-01-01,2023-12-31]")  # type: ignore[arg-type]

    def test_daterange_formats_supported(self) -> None:
        """Test that various DateRange formats are supported."""
        path_sql = SQL("data->>'period'")

        # Test all valid PostgreSQL daterange formats
        valid_ranges = [
            "[2023-01-01,2023-12-31]",  # Both inclusive
            "[2023-01-01,2023-12-31)",  # Start inclusive, end exclusive
            "(2023-01-01,2023-12-31]",  # Start exclusive, end inclusive
            "(2023-01-01,2023-12-31)",  # Both exclusive
            "[2023-01-01,)",  # Unbounded end
            "(,2023-12-31]",  # Unbounded start
        ]

        for date_range in valid_ranges:
            result = build_daterange_eq_sql(path_sql, date_range)
            expected = f"(data->>'period')::daterange = '{date_range}'::daterange"
            assert result.as_string(None) == expected

    def test_daterange_infinite_bounds(self) -> None:
        """Test DateRange with infinite bounds."""
        path_sql = SQL("data->>'period'")

        # Test infinity
        result_inf = build_daterange_eq_sql(path_sql, "[2023-01-01,infinity)")
        expected_inf = "(data->>'period')::daterange = '[2023-01-01,infinity)'::daterange"
        assert result_inf.as_string(None) == expected_inf

        # Test negative infinity
        result_neg_inf = build_daterange_eq_sql(path_sql, "(-infinity,2023-12-31]")
        expected_neg_inf = "(data->>'period')::daterange = '(-infinity,2023-12-31]'::daterange"
        assert result_neg_inf.as_string(None) == expected_neg_inf
