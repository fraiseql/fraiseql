"""Tests for DateRange operators SQL building functions.

These tests verify that DateRange operators generate correct PostgreSQL SQL
with proper daterange casting for temporal range operations.
"""

import pytest
from psycopg.sql import SQL

# Import DateRange operator functions
from fraiseql.sql.where.operators.date_range import (
    build_daterange_eq_sql,
    build_daterange_neq_sql,
    build_daterange_in_sql,
    build_daterange_notin_sql,
    build_contains_date_sql,
    build_overlaps_sql,
    build_adjacent_sql,
    build_strictly_left_sql,
    build_strictly_right_sql,
    build_not_left_sql,
    build_not_right_sql,
)


class TestDateRangeBasicOperators:
    """Test basic DateRange operators (eq, neq, in, notin)."""

    def test_build_daterange_equality_sql(self):
        """Test DateRange equality operator with proper daterange casting."""
        path_sql = SQL("data->>'period'")
        value = "[2023-01-01,2023-12-31]"

        result = build_daterange_eq_sql(path_sql, value)
        expected = "(data->>'period')::daterange = '[2023-01-01,2023-12-31]'::daterange"

        assert result.as_string(None) == expected

    def test_build_daterange_inequality_sql(self):
        """Test DateRange inequality operator with proper daterange casting."""
        path_sql = SQL("data->>'period'")
        value = "[2023-01-01,2023-12-31]"

        result = build_daterange_neq_sql(path_sql, value)
        expected = "(data->>'period')::daterange != '[2023-01-01,2023-12-31]'::daterange"

        assert result.as_string(None) == expected

    def test_build_daterange_in_list_sql(self):
        """Test DateRange IN list with multiple daterange values."""
        path_sql = SQL("data->>'period'")
        value = ["[2023-01-01,2023-12-31]", "[2024-01-01,2024-12-31]"]

        result = build_daterange_in_sql(path_sql, value)
        expected = "(data->>'period')::daterange IN ('[2023-01-01,2023-12-31]'::daterange, '[2024-01-01,2024-12-31]'::daterange)"

        assert result.as_string(None) == expected

    def test_build_daterange_not_in_list_sql(self):
        """Test DateRange NOT IN list with multiple daterange values."""
        path_sql = SQL("data->>'period'")
        value = ["[2023-01-01,2023-12-31]", "[2024-01-01,2024-12-31]"]

        result = build_daterange_notin_sql(path_sql, value)
        expected = "(data->>'period')::daterange NOT IN ('[2023-01-01,2023-12-31]'::daterange, '[2024-01-01,2024-12-31]'::daterange)"

        assert result.as_string(None) == expected

    def test_build_daterange_single_item_in_list(self):
        """Test DateRange IN list with single value."""
        path_sql = SQL("data->>'period'")
        value = ["[2023-01-01,2023-12-31]"]

        result = build_daterange_in_sql(path_sql, value)
        expected = "(data->>'period')::daterange IN ('[2023-01-01,2023-12-31]'::daterange)"

        assert result.as_string(None) == expected

    def test_build_daterange_different_bracket_types(self):
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

    def test_build_daterange_empty_list_handling(self):
        """Test DateRange operators handle empty lists gracefully."""
        path_sql = SQL("data->>'period'")
        value = []

        result_in = build_daterange_in_sql(path_sql, value)
        expected_in = "(data->>'period')::daterange IN ()"
        assert result_in.as_string(None) == expected_in

        result_notin = build_daterange_notin_sql(path_sql, value)
        expected_notin = "(data->>'period')::daterange NOT IN ()"
        assert result_notin.as_string(None) == expected_notin


class TestDateRangeSpecificOperators:
    """Test DateRange-specific operators for range operations."""

    def test_build_contains_date_sql(self):
        """Test DateRange contains_date (@>) operator."""
        path_sql = SQL("data->>'period'")
        value = "2023-07-15"

        result = build_contains_date_sql(path_sql, value)
        expected = "(data->>'period')::daterange @> '2023-07-15'"

        assert result.as_string(None) == expected

    def test_build_overlaps_sql(self):
        """Test DateRange overlaps (&&) operator."""
        path_sql = SQL("data->>'period'")
        value = "[2023-06-01,2023-08-31]"

        result = build_overlaps_sql(path_sql, value)
        expected = "(data->>'period')::daterange && '[2023-06-01,2023-08-31]'::daterange"

        assert result.as_string(None) == expected

    def test_build_adjacent_sql(self):
        """Test DateRange adjacent (-|-) operator."""
        path_sql = SQL("data->>'period'")
        value = "[2024-01-01,2024-12-31]"

        result = build_adjacent_sql(path_sql, value)
        expected = "(data->>'period')::daterange -|- '[2024-01-01,2024-12-31]'::daterange"

        assert result.as_string(None) == expected

    def test_build_strictly_left_sql(self):
        """Test DateRange strictly_left (<<) operator."""
        path_sql = SQL("data->>'period'")
        value = "[2024-01-01,2024-12-31]"

        result = build_strictly_left_sql(path_sql, value)
        expected = "(data->>'period')::daterange << '[2024-01-01,2024-12-31]'::daterange"

        assert result.as_string(None) == expected

    def test_build_strictly_right_sql(self):
        """Test DateRange strictly_right (>>) operator."""
        path_sql = SQL("data->>'period'")
        value = "[2022-01-01,2022-12-31]"

        result = build_strictly_right_sql(path_sql, value)
        expected = "(data->>'period')::daterange >> '[2022-01-01,2022-12-31]'::daterange"

        assert result.as_string(None) == expected

    def test_build_not_left_sql(self):
        """Test DateRange not_left (&>) operator."""
        path_sql = SQL("data->>'period'")
        value = "[2023-01-01,2023-12-31]"

        result = build_not_left_sql(path_sql, value)
        expected = "(data->>'period')::daterange &> '[2023-01-01,2023-12-31]'::daterange"

        assert result.as_string(None) == expected

    def test_build_not_right_sql(self):
        """Test DateRange not_right (&<) operator."""
        path_sql = SQL("data->>'period'")
        value = "[2023-01-01,2023-12-31]"

        result = build_not_right_sql(path_sql, value)
        expected = "(data->>'period')::daterange &< '[2023-01-01,2023-12-31]'::daterange"

        assert result.as_string(None) == expected

    def test_range_operators_complex_ranges(self):
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


class TestDateRangeValidation:
    """Test DateRange operator validation and error handling."""

    def test_daterange_in_requires_list(self):
        """Test that DateRange 'in' operator requires a list."""
        path_sql = SQL("data->>'period'")

        with pytest.raises(TypeError, match="'in' operator requires a list"):
            build_daterange_in_sql(path_sql, "[2023-01-01,2023-12-31]")

    def test_daterange_notin_requires_list(self):
        """Test that DateRange 'notin' operator requires a list."""
        path_sql = SQL("data->>'period'")

        with pytest.raises(TypeError, match="'notin' operator requires a list"):
            build_daterange_notin_sql(path_sql, "[2023-01-01,2023-12-31]")

    def test_daterange_formats_supported(self):
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

    def test_daterange_infinite_bounds(self):
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
