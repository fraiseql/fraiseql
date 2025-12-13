"""End-to-end tests for DateTime/Date filtering functionality.

These tests verify that DateTime and Date operators work correctly in the full context
of the WHERE clause building system, from field detection through SQL generation.
"""

import pytest
from psycopg.sql import SQL

from fraiseql.sql.where.core.field_detection import FieldType
from fraiseql.sql.where.operators import get_operator_function

pytestmark = pytest.mark.database


class TestDateTimeEndToEndIntegration:
    """Test DateTime operators in full integration context."""

    def test_datetime_field_type_operators(self) -> None:
        """Test that all expected DateTime operators are available."""
        expected_operators = {"eq", "neq", "in", "notin", "gt", "gte", "lt", "lte"}

        for op in expected_operators:
            func = get_operator_function(FieldType.DATETIME, op)
            assert callable(func), f"DateTime operator '{op}' should return a callable function"

    def test_datetime_operators_integration(self) -> None:
        """Test DateTime operators generate correct SQL in full context."""
        path_sql = SQL("data->>'created_at'")

        # Test equality
        eq_func = get_operator_function(FieldType.DATETIME, "eq")
        result = eq_func(path_sql, "2023-07-15T14:30:00Z")
        expected = "(data->>'created_at')::timestamptz = '2023-07-15T14:30:00Z'::timestamptz"
        assert result.as_string(None) == expected

        # Test IN list
        in_func = get_operator_function(FieldType.DATETIME, "in")
        result = in_func(path_sql, ["2023-07-15T10:00:00Z", "2023-07-15T14:30:00Z"])
        expected = "(data->>'created_at')::timestamptz IN ('2023-07-15T10:00:00Z'::timestamptz, '2023-07-15T14:30:00Z'::timestamptz)"
        assert result.as_string(None) == expected

        # Test greater than
        gt_func = get_operator_function(FieldType.DATETIME, "gt")
        result = gt_func(path_sql, "2023-07-15T09:00:00Z")
        expected = "(data->>'created_at')::timestamptz > '2023-07-15T09:00:00Z'::timestamptz"
        assert result.as_string(None) == expected

    def test_datetime_timezone_formats_integration(self) -> None:
        """Test DateTime with various timezone formats in integration."""
        path_sql = SQL("data->>'timestamp'")
        eq_func = get_operator_function(FieldType.DATETIME, "eq")

        # Test different timezone formats
        test_cases = [
            (
                "2023-07-15T14:30:00Z",
                "(data->>'timestamp')::timestamptz = '2023-07-15T14:30:00Z'::timestamptz",
            ),
            (
                "2023-07-15T14:30:00+02:00",
                "(data->>'timestamp')::timestamptz = '2023-07-15T14:30:00+02:00'::timestamptz",
            ),
            (
                "2023-07-15T14:30:00-05:00",
                "(data->>'timestamp')::timestamptz = '2023-07-15T14:30:00-05:00'::timestamptz",
            ),
            (
                "2023-07-15T14:30:00.123Z",
                "(data->>'timestamp')::timestamptz = '2023-07-15T14:30:00.123Z'::timestamptz",
            ),
        ]

        for datetime_str, expected in test_cases:
            result = eq_func(path_sql, datetime_str)
            assert result.as_string(None) == expected

    def test_datetime_comparison_operators_integration(self) -> None:
        """Test DateTime comparison operators in integration context."""
        path_sql = SQL("data->>'event_time'")

        # Test all comparison operators
        comparisons = [
            (
                "gte",
                "2023-07-15T00:00:00Z",
                "(data->>'event_time')::timestamptz >= '2023-07-15T00:00:00Z'::timestamptz",
            ),
            (
                "lt",
                "2023-07-16T00:00:00Z",
                "(data->>'event_time')::timestamptz < '2023-07-16T00:00:00Z'::timestamptz",
            ),
            (
                "lte",
                "2023-07-15T23:59:59Z",
                "(data->>'event_time')::timestamptz <= '2023-07-15T23:59:59Z'::timestamptz",
            ),
        ]

        for op, datetime_str, expected in comparisons:
            func = get_operator_function(FieldType.DATETIME, op)
            result = func(path_sql, datetime_str)
            assert result.as_string(None) == expected


class TestDateEndToEndIntegration:
    """Test Date operators in full integration context."""

    def test_date_field_type_operators(self) -> None:
        """Test that all expected Date operators are available."""
        expected_operators = {"eq", "neq", "in", "notin", "gt", "gte", "lt", "lte"}

        for op in expected_operators:
            func = get_operator_function(FieldType.DATE, op)
            assert callable(func), f"Date operator '{op}' should return a callable function"

    def test_date_operators_integration(self) -> None:
        """Test Date operators generate correct SQL in full context."""
        path_sql = SQL("data->>'birth_date'")

        # Test equality
        eq_func = get_operator_function(FieldType.DATE, "eq")
        result = eq_func(path_sql, "2023-07-15")
        expected = "(data->>'birth_date')::date = '2023-07-15'::date"
        assert result.as_string(None) == expected

        # Test IN list
        in_func = get_operator_function(FieldType.DATE, "in")
        result = in_func(path_sql, ["2023-07-04", "2023-12-25", "2023-01-01"])
        expected = "(data->>'birth_date')::date IN ('2023-07-04'::date, '2023-12-25'::date, '2023-01-01'::date)"
        assert result.as_string(None) == expected

        # Test greater than
        gt_func = get_operator_function(FieldType.DATE, "gt")
        result = gt_func(path_sql, "2023-07-01")
        expected = "(data->>'birth_date')::date > '2023-07-01'::date"
        assert result.as_string(None) == expected

    def test_date_comparison_operators_integration(self) -> None:
        """Test Date comparison operators in integration context."""
        path_sql = SQL("data->>'event_date'")

        # Test all comparison operators
        comparisons = [
            ("gte", "2023-07-01", "(data->>'event_date')::date >= '2023-07-01'::date"),
            ("lte", "2023-07-31", "(data->>'event_date')::date <= '2023-07-31'::date"),
            ("lt", "2024-01-01", "(data->>'event_date')::date < '2024-01-01'::date"),
        ]

        for op, date_str, expected in comparisons:
            func = get_operator_function(FieldType.DATE, op)
            result = func(path_sql, date_str)
            assert result.as_string(None) == expected

    def test_date_special_formats_integration(self) -> None:
        """Test Date with special date formats in integration."""
        path_sql = SQL("data->>'date'")
        eq_func = get_operator_function(FieldType.DATE, "eq")

        # Test various date formats
        test_cases = [
            ("2024-02-29", "(data->>'date')::date = '2024-02-29'::date"),  # Leap year
            ("2023-01-31", "(data->>'date')::date = '2023-01-31'::date"),  # End of January
            ("2023-04-30", "(data->>'date')::date = '2023-04-30'::date"),  # End of April
            ("2023-12-31", "(data->>'date')::date = '2023-12-31'::date"),  # End of year
        ]

        for date_str, expected in test_cases:
            result = eq_func(path_sql, date_str)
            assert result.as_string(None) == expected


class TestDateTimeVsDateOperators:
    """Test that DateTime and Date operators are properly differentiated."""

    def test_datetime_uses_timestamptz_casting(self) -> None:
        """Verify DateTime operators use ::timestamptz casting."""
        path_sql = SQL("data->>'created_at'")
        eq_func = get_operator_function(FieldType.DATETIME, "eq")
        result = eq_func(path_sql, "2023-07-15T14:30:00Z")
        query_str = result.as_string(None)

        # Should contain ::timestamptz casting
        assert "::timestamptz" in query_str
        assert "::date" not in query_str

    def test_date_uses_date_casting(self) -> None:
        """Verify Date operators use ::date casting."""
        path_sql = SQL("data->>'birth_date'")
        eq_func = get_operator_function(FieldType.DATE, "eq")
        result = eq_func(path_sql, "2023-07-15")
        query_str = result.as_string(None)

        # Should contain ::date casting
        assert "::date" in query_str
        assert "::timestamptz" not in query_str

    def test_operator_coverage_datetime(self) -> None:
        """Test that all expected DateTime operators are available."""
        from fraiseql.sql.where.operators import OPERATOR_MAP

        datetime_operators = [
            (FieldType.DATETIME, op)
            for op in ["eq", "neq", "in_", "in", "notin", "gt", "gte", "lt", "lte"]
        ]

        for key in datetime_operators:
            assert key in OPERATOR_MAP, f"Missing DateTime operator: {key[1]}"

    def test_operator_coverage_date(self) -> None:
        """Test that all expected Date operators are available."""
        from fraiseql.sql.where.operators import OPERATOR_MAP

        date_operators = [
            (FieldType.DATE, op)
            for op in ["eq", "neq", "in_", "in", "notin", "gt", "gte", "lt", "lte"]
        ]

        for key in date_operators:
            assert key in OPERATOR_MAP, f"Missing Date operator: {key[1]}"


class TestDateTimeErrorHandling:
    """Test error handling for DateTime/Date operators."""

    def test_datetime_invalid_operator_error(self) -> None:
        """Test that invalid DateTime operators raise appropriate errors."""
        with pytest.raises(ValueError, match="Unsupported operator 'invalid_op'"):
            get_operator_function(FieldType.DATETIME, "invalid_op")

    def test_date_invalid_operator_error(self) -> None:
        """Test that invalid Date operators raise appropriate errors."""
        with pytest.raises(ValueError, match="Unsupported operator 'invalid_op'"):
            get_operator_function(FieldType.DATE, "invalid_op")

    def test_datetime_in_requires_list_error(self) -> None:
        """Test DateTime 'in' operator with non-list value raises error."""
        path_sql = SQL("data->>'timestamp'")
        in_func = get_operator_function(FieldType.DATETIME, "in")

        with pytest.raises(TypeError, match="'in' operator requires a list"):
            in_func(path_sql, "2023-07-15T14:30:00Z")  # Should be a list

    def test_date_in_requires_list_error(self) -> None:
        """Test Date 'in' operator with non-list value raises error."""
        path_sql = SQL("data->>'date'")
        in_func = get_operator_function(FieldType.DATE, "in")

        with pytest.raises(TypeError, match="'in' operator requires a list"):
            in_func(path_sql, "2023-07-15")  # Should be a list
