"""Comprehensive tests for datetime operator SQL building.

Consolidated from test_datetime_operators_sql_building.py and datetime parts of test_date_datetime_port_complete.py.
"""

import pytest
from psycopg.sql import SQL

from fraiseql.sql.where.operators.datetime import (
    build_datetime_eq_sql,
    build_datetime_gt_sql,
    build_datetime_gte_sql,
    build_datetime_in_sql,
    build_datetime_lt_sql,
    build_datetime_lte_sql,
    build_datetime_neq_sql,
    build_datetime_notin_sql,
)


class TestDateTimeBasicOperators:
    """Test basic DateTime operators (eq, neq, in, notin)."""

    def test_datetime_eq(self):
        """Test datetime equality operator."""
        path_sql = SQL("data->>'created_at'")
        result = build_datetime_eq_sql(path_sql, "2023-07-15T14:30:00Z")
        sql_str = result.as_string(None)
        assert sql_str == "(data->>'created_at')::timestamptz = '2023-07-15T14:30:00Z'::timestamptz"

    def test_datetime_neq(self):
        """Test datetime inequality operator."""
        path_sql = SQL("data->>'modified_at'")
        result = build_datetime_neq_sql(path_sql, "2023-07-15T14:30:00Z")
        sql_str = result.as_string(None)
        assert (
            sql_str == "(data->>'modified_at')::timestamptz != '2023-07-15T14:30:00Z'::timestamptz"
        )

    def test_datetime_in(self):
        """Test datetime IN operator."""
        path_sql = SQL("data->>'event_time'")
        result = build_datetime_in_sql(path_sql, ["2023-01-01T00:00:00Z", "2023-12-31T23:59:59Z"])
        sql_str = result.as_string(None)
        expected = "(data->>'event_time')::timestamptz IN ('2023-01-01T00:00:00Z'::timestamptz, '2023-12-31T23:59:59Z'::timestamptz)"
        assert expected == sql_str

    def test_datetime_notin(self):
        """Test datetime NOT IN operator."""
        path_sql = SQL("data->>'excluded_time'")
        result = build_datetime_notin_sql(
            path_sql, ["2023-01-01T00:00:00Z", "2023-12-25T12:00:00Z"]
        )
        sql_str = result.as_string(None)
        expected = "(data->>'excluded_time')::timestamptz NOT IN ('2023-01-01T00:00:00Z'::timestamptz, '2023-12-25T12:00:00Z'::timestamptz)"
        assert expected == sql_str

    def test_build_datetime_equality_sql(self) -> None:
        """Test DateTime equality operator with proper timestamp casting."""
        path_sql = SQL("data->>'created_at'")
        value = "2023-07-15T14:30:00Z"

        result = build_datetime_eq_sql(path_sql, value)
        expected = "(data->>'created_at')::timestamptz = '2023-07-15T14:30:00Z'::timestamptz"

        assert result.as_string(None) == expected

    def test_build_datetime_inequality_sql(self) -> None:
        """Test DateTime inequality operator with proper timestamp casting."""
        path_sql = SQL("data->>'updated_at'")
        value = "2023-01-01T00:00:00Z"

        result = build_datetime_neq_sql(path_sql, value)
        expected = "(data->>'updated_at')::timestamptz != '2023-01-01T00:00:00Z'::timestamptz"

        assert result.as_string(None) == expected

    def test_build_datetime_in_list_sql(self) -> None:
        """Test DateTime IN list with multiple datetime values."""
        path_sql = SQL("data->>'event_time'")
        value = ["2023-07-15T10:00:00Z", "2023-07-15T14:30:00Z", "2023-07-15T18:00:00Z"]

        result = build_datetime_in_sql(path_sql, value)
        expected = "(data->>'event_time')::timestamptz IN ('2023-07-15T10:00:00Z'::timestamptz, '2023-07-15T14:30:00Z'::timestamptz, '2023-07-15T18:00:00Z'::timestamptz)"

        assert result.as_string(None) == expected

    def test_build_datetime_not_in_list_sql(self) -> None:
        """Test DateTime NOT IN list with multiple datetime values."""
        path_sql = SQL("data->>'event_time'")
        value = ["2023-01-01T00:00:00Z", "2023-12-31T23:59:59Z"]

        result = build_datetime_notin_sql(path_sql, value)
        expected = "(data->>'event_time')::timestamptz NOT IN ('2023-01-01T00:00:00Z'::timestamptz, '2023-12-31T23:59:59Z'::timestamptz)"

        assert result.as_string(None) == expected

    def test_build_datetime_single_item_in_list(self) -> None:
        """Test DateTime IN list with single value."""
        path_sql = SQL("data->>'timestamp'")
        value = ["2023-07-15T14:30:00Z"]

        result = build_datetime_in_sql(path_sql, value)
        expected = "(data->>'timestamp')::timestamptz IN ('2023-07-15T14:30:00Z'::timestamptz)"

        assert result.as_string(None) == expected

    def test_build_datetime_timezone_formats(self) -> None:
        """Test DateTime operators with different timezone formats."""
        path_sql = SQL("data->>'timestamp'")

        # Test UTC with Z suffix
        result_utc = build_datetime_eq_sql(path_sql, "2023-07-15T14:30:00Z")
        expected_utc = "(data->>'timestamp')::timestamptz = '2023-07-15T14:30:00Z'::timestamptz"
        assert result_utc.as_string(None) == expected_utc

        # Test with offset
        result_offset = build_datetime_eq_sql(path_sql, "2023-07-15T14:30:00+02:00")
        expected_offset = (
            "(data->>'timestamp')::timestamptz = '2023-07-15T14:30:00+02:00'::timestamptz"
        )
        assert result_offset.as_string(None) == expected_offset

        # Test negative offset
        result_neg_offset = build_datetime_eq_sql(path_sql, "2023-07-15T14:30:00-05:00")
        expected_neg_offset = (
            "(data->>'timestamp')::timestamptz = '2023-07-15T14:30:00-05:00'::timestamptz"
        )
        assert result_neg_offset.as_string(None) == expected_neg_offset

    def test_build_datetime_empty_list_handling(self) -> None:
        """Test DateTime operators handle empty lists gracefully."""
        path_sql = SQL("data->>'timestamp'")
        value = []

        result_in = build_datetime_in_sql(path_sql, value)
        expected_in = "(data->>'timestamp')::timestamptz IN ()"
        assert result_in.as_string(None) == expected_in

        result_notin = build_datetime_notin_sql(path_sql, value)
        expected_notin = "(data->>'timestamp')::timestamptz NOT IN ()"
        assert result_notin.as_string(None) == expected_notin


class TestDateTimeComparisonOperators:
    """Test DateTime comparison operators (gt, gte, lt, lte)."""

    def test_datetime_gt(self):
        """Test datetime greater than operator."""
        path_sql = SQL("data->>'created_at'")
        result = build_datetime_gt_sql(path_sql, "2023-01-01T00:00:00Z")
        sql_str = result.as_string(None)
        assert sql_str == "(data->>'created_at')::timestamptz > '2023-01-01T00:00:00Z'::timestamptz"

    def test_datetime_gte(self):
        """Test datetime greater than or equal operator."""
        path_sql = SQL("data->>'start_time'")
        result = build_datetime_gte_sql(path_sql, "2023-06-01T12:00:00Z")
        sql_str = result.as_string(None)
        assert (
            sql_str == "(data->>'start_time')::timestamptz >= '2023-06-01T12:00:00Z'::timestamptz"
        )

    def test_datetime_lt(self):
        """Test datetime less than operator."""
        path_sql = SQL("data->>'expires_at'")
        result = build_datetime_lt_sql(path_sql, "2024-12-31T23:59:59Z")
        sql_str = result.as_string(None)
        assert sql_str == "(data->>'expires_at')::timestamptz < '2024-12-31T23:59:59Z'::timestamptz"

    def test_datetime_lte(self):
        """Test datetime less than or equal operator."""
        path_sql = SQL("data->>'deadline'")
        result = build_datetime_lte_sql(path_sql, "2023-12-31T23:59:59Z")
        sql_str = result.as_string(None)
        assert sql_str == "(data->>'deadline')::timestamptz <= '2023-12-31T23:59:59Z'::timestamptz"

    def test_build_datetime_greater_than_sql(self) -> None:
        """Test DateTime greater than operator."""
        path_sql = SQL("data->>'created_at'")
        value = "2023-07-01T00:00:00Z"

        result = build_datetime_gt_sql(path_sql, value)
        expected = "(data->>'created_at')::timestamptz > '2023-07-01T00:00:00Z'::timestamptz"

        assert result.as_string(None) == expected

    def test_build_datetime_greater_than_equal_sql(self) -> None:
        """Test DateTime greater than or equal operator."""
        path_sql = SQL("data->>'created_at'")
        value = "2023-07-01T00:00:00Z"

        result = build_datetime_gte_sql(path_sql, value)
        expected = "(data->>'created_at')::timestamptz >= '2023-07-01T00:00:00Z'::timestamptz"

        assert result.as_string(None) == expected

    def test_build_datetime_less_than_sql(self) -> None:
        """Test DateTime less than operator."""
        path_sql = SQL("data->>'created_at'")
        value = "2023-12-31T23:59:59Z"

        result = build_datetime_lt_sql(path_sql, value)
        expected = "(data->>'created_at')::timestamptz < '2023-12-31T23:59:59Z'::timestamptz"

        assert result.as_string(None) == expected

    def test_build_datetime_less_than_equal_sql(self) -> None:
        """Test DateTime less than or equal operator."""
        path_sql = SQL("data->>'created_at'")
        value = "2023-12-31T23:59:59Z"

        result = build_datetime_lte_sql(path_sql, value)
        expected = "(data->>'created_at')::timestamptz <= '2023-12-31T23:59:59Z'::timestamptz"

        assert result.as_string(None) == expected

    def test_datetime_range_queries(self) -> None:
        """Test DateTime range queries with comparison operators."""
        path_sql = SQL("data->>'event_time'")

        # Test start of day
        result_start = build_datetime_gte_sql(path_sql, "2023-07-15T00:00:00Z")
        expected_start = "(data->>'event_time')::timestamptz >= '2023-07-15T00:00:00Z'::timestamptz"
        assert result_start.as_string(None) == expected_start

        # Test end of day
        result_end = build_datetime_lt_sql(path_sql, "2023-07-16T00:00:00Z")
        expected_end = "(data->>'event_time')::timestamptz < '2023-07-16T00:00:00Z'::timestamptz"
        assert result_end.as_string(None) == expected_end

        # Test business hours start
        result_business_start = build_datetime_gte_sql(path_sql, "2023-07-15T09:00:00Z")
        expected_business_start = (
            "(data->>'event_time')::timestamptz >= '2023-07-15T09:00:00Z'::timestamptz"
        )
        assert result_business_start.as_string(None) == expected_business_start

        # Test business hours end
        result_business_end = build_datetime_lte_sql(path_sql, "2023-07-15T17:00:00Z")
        expected_business_end = (
            "(data->>'event_time')::timestamptz <= '2023-07-15T17:00:00Z'::timestamptz"
        )
        assert result_business_end.as_string(None) == expected_business_end


class TestDateTimeValidation:
    """Test DateTime operator validation and error handling."""

    def test_datetime_in_requires_list(self) -> None:
        """Test that DateTime 'in' operator requires a list."""
        path_sql = SQL("data->>'timestamp'")

        with pytest.raises(TypeError, match="'in' operator requires a list"):
            build_datetime_in_sql(path_sql, "2023-07-15T14:30:00Z")  # type: ignore[arg-type]

    def test_datetime_notin_requires_list(self) -> None:
        """Test that DateTime 'notin' operator requires a list."""
        path_sql = SQL("data->>'timestamp'")

        with pytest.raises(TypeError, match="'notin' operator requires a list"):
            build_datetime_notin_sql(path_sql, "2023-07-15T14:30:00Z")  # type: ignore[arg-type]

    def test_datetime_iso_formats_supported(self) -> None:
        """Test that various ISO 8601 datetime formats are supported."""
        path_sql = SQL("data->>'timestamp'")

        # Test various valid ISO 8601 datetime formats
        valid_datetimes = [
            "2023-07-15T14:30:00Z",  # UTC with Z
            "2023-07-15T14:30:00+00:00",  # UTC with offset
            "2023-07-15T14:30:00+02:00",  # Positive offset
            "2023-07-15T14:30:00-05:00",  # Negative offset
            "2023-07-15T14:30:00.123Z",  # With milliseconds
            "2023-07-15T14:30:00.123456Z",  # With microseconds
            "2023-12-31T23:59:59Z",  # End of year
            "2023-01-01T00:00:00Z",  # Start of year
        ]

        for datetime_str in valid_datetimes:
            result = build_datetime_eq_sql(path_sql, datetime_str)
            expected = f"(data->>'timestamp')::timestamptz = '{datetime_str}'::timestamptz"
            assert result.as_string(None) == expected

    def test_datetime_precision_handling(self) -> None:
        """Test DateTime with different precision levels."""
        path_sql = SQL("data->>'timestamp'")

        # Test second precision
        result_seconds = build_datetime_eq_sql(path_sql, "2023-07-15T14:30:00Z")
        expected_seconds = "(data->>'timestamp')::timestamptz = '2023-07-15T14:30:00Z'::timestamptz"
        assert result_seconds.as_string(None) == expected_seconds

        # Test millisecond precision
        result_millis = build_datetime_eq_sql(path_sql, "2023-07-15T14:30:00.123Z")
        expected_millis = (
            "(data->>'timestamp')::timestamptz = '2023-07-15T14:30:00.123Z'::timestamptz"
        )
        assert result_millis.as_string(None) == expected_millis

        # Test microsecond precision
        result_micros = build_datetime_eq_sql(path_sql, "2023-07-15T14:30:00.123456Z")
        expected_micros = (
            "(data->>'timestamp')::timestamptz = '2023-07-15T14:30:00.123456Z'::timestamptz"
        )
        assert result_micros.as_string(None) == expected_micros

    def test_datetime_timezone_edge_cases(self) -> None:
        """Test DateTime with timezone edge cases."""
        path_sql = SQL("data->>'timestamp'")

        # Test maximum positive offset
        result_max_pos = build_datetime_eq_sql(path_sql, "2023-07-15T14:30:00+14:00")
        expected_max_pos = (
            "(data->>'timestamp')::timestamptz = '2023-07-15T14:30:00+14:00'::timestamptz"
        )
        assert result_max_pos.as_string(None) == expected_max_pos

        # Test maximum negative offset
        result_max_neg = build_datetime_eq_sql(path_sql, "2023-07-15T14:30:00-12:00")
        expected_max_neg = (
            "(data->>'timestamp')::timestamptz = '2023-07-15T14:30:00-12:00'::timestamptz"
        )
        assert result_max_neg.as_string(None) == expected_max_neg
