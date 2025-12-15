"""Comprehensive tests for date operator SQL building.

Consolidated from test_date_operators_sql_building.py and date parts of test_date_datetime_port_complete.py.
"""

import pytest
from psycopg.sql import SQL

from fraiseql.sql.where.operators.date import (
    build_date_eq_sql,
    build_date_gt_sql,
    build_date_gte_sql,
    build_date_in_sql,
    build_date_lt_sql,
    build_date_lte_sql,
    build_date_neq_sql,
    build_date_notin_sql,
)


class TestDateBasicOperators:
    """Test basic Date operators (eq, neq, in, notin)."""

    def test_date_eq(self):
        """Test date equality operator."""
        path_sql = SQL("data->>'birth_date'")
        result = build_date_eq_sql(path_sql, "2023-07-15")
        sql_str = result.as_string(None)
        assert sql_str == "(data->>'birth_date')::date = '2023-07-15'::date"

    def test_date_neq(self):
        """Test date inequality operator."""
        path_sql = SQL("data->>'birth_date'")
        result = build_date_neq_sql(path_sql, "2023-07-15")
        sql_str = result.as_string(None)
        assert sql_str == "(data->>'birth_date')::date != '2023-07-15'::date"

    def test_date_in(self):
        """Test date IN operator."""
        path_sql = SQL("data->>'event_date'")
        result = build_date_in_sql(path_sql, ["2023-01-01", "2023-12-31"])
        sql_str = result.as_string(None)
        expected = "(data->>'event_date')::date IN ('2023-01-01'::date, '2023-12-31'::date)"
        assert expected == sql_str

    def test_date_notin(self):
        """Test date NOT IN operator."""
        path_sql = SQL("data->>'excluded_date'")
        result = build_date_notin_sql(path_sql, ["2023-01-01", "2023-12-25"])
        sql_str = result.as_string(None)
        expected = "(data->>'excluded_date')::date NOT IN ('2023-01-01'::date, '2023-12-25'::date)"
        assert expected == sql_str

    def test_build_date_equality_sql(self) -> None:
        """Test Date equality operator with proper date casting."""
        path_sql = SQL("data->>'birth_date'")
        value = "2023-07-15"

        result = build_date_eq_sql(path_sql, value)
        expected = "(data->>'birth_date')::date = '2023-07-15'::date"

        assert result.as_string(None) == expected

    def test_build_date_inequality_sql(self) -> None:
        """Test Date inequality operator with proper date casting."""
        path_sql = SQL("data->>'event_date'")
        value = "2023-01-01"

        result = build_date_neq_sql(path_sql, value)
        expected = "(data->>'event_date')::date != '2023-01-01'::date"

        assert result.as_string(None) == expected

    def test_build_date_in_list_sql(self) -> None:
        """Test Date IN list with multiple date values."""
        path_sql = SQL("data->>'holiday_date'")
        value = ["2023-07-04", "2023-12-25", "2023-01-01"]

        result = build_date_in_sql(path_sql, value)
        expected = "(data->>'holiday_date')::date IN ('2023-07-04'::date, '2023-12-25'::date, '2023-01-01'::date)"

        assert result.as_string(None) == expected

    def test_build_date_not_in_list_sql(self) -> None:
        """Test Date NOT IN list with multiple date values."""
        path_sql = SQL("data->>'work_date'")
        value = ["2023-07-04", "2023-12-25"]

        result = build_date_notin_sql(path_sql, value)
        expected = "(data->>'work_date')::date NOT IN ('2023-07-04'::date, '2023-12-25'::date)"

        assert result.as_string(None) == expected

    def test_build_date_single_item_in_list(self) -> None:
        """Test Date IN list with single value."""
        path_sql = SQL("data->>'anniversary'")
        value = ["2023-07-15"]

        result = build_date_in_sql(path_sql, value)
        expected = "(data->>'anniversary')::date IN ('2023-07-15'::date)"

        assert result.as_string(None) == expected

    def test_build_date_formats(self) -> None:
        """Test Date operators with different date formats."""
        path_sql = SQL("data->>'date'")

        # Test standard ISO date
        result_iso = build_date_eq_sql(path_sql, "2023-07-15")
        expected_iso = "(data->>'date')::date = '2023-07-15'::date"
        assert result_iso.as_string(None) == expected_iso

        # Test start of year
        result_start = build_date_eq_sql(path_sql, "2023-01-01")
        expected_start = "(data->>'date')::date = '2023-01-01'::date"
        assert result_start.as_string(None) == expected_start

        # Test end of year
        result_end = build_date_eq_sql(path_sql, "2023-12-31")
        expected_end = "(data->>'date')::date = '2023-12-31'::date"
        assert result_end.as_string(None) == expected_end

    def test_build_date_empty_list_handling(self) -> None:
        """Test Date operators handle empty lists gracefully."""
        path_sql = SQL("data->>'date'")
        value = []

        result_in = build_date_in_sql(path_sql, value)
        expected_in = "(data->>'date')::date IN ()"
        assert result_in.as_string(None) == expected_in

        result_notin = build_date_notin_sql(path_sql, value)
        expected_notin = "(data->>'date')::date NOT IN ()"
        assert result_notin.as_string(None) == expected_notin


class TestDateComparisonOperators:
    """Test Date comparison operators (gt, gte, lt, lte)."""

    def test_date_gt(self):
        """Test date greater than operator."""
        path_sql = SQL("data->>'created_date'")
        result = build_date_gt_sql(path_sql, "2023-01-01")
        sql_str = result.as_string(None)
        assert sql_str == "(data->>'created_date')::date > '2023-01-01'::date"

    def test_date_gte(self):
        """Test date greater than or equal operator."""
        path_sql = SQL("data->>'start_date'")
        result = build_date_gte_sql(path_sql, "2023-06-01")
        sql_str = result.as_string(None)
        assert sql_str == "(data->>'start_date')::date >= '2023-06-01'::date"

    def test_date_lt(self):
        """Test date less than operator."""
        path_sql = SQL("data->>'expiry_date'")
        result = build_date_lt_sql(path_sql, "2024-12-31")
        sql_str = result.as_string(None)
        assert sql_str == "(data->>'expiry_date')::date < '2024-12-31'::date"

    def test_date_lte(self):
        """Test date less than or equal operator."""
        path_sql = SQL("data->>'deadline'")
        result = build_date_lte_sql(path_sql, "2023-12-31")
        sql_str = result.as_string(None)
        assert sql_str == "(data->>'deadline')::date <= '2023-12-31'::date"

    def test_build_date_greater_than_sql(self) -> None:
        """Test Date greater than operator."""
        path_sql = SQL("data->>'start_date'")
        value = "2023-07-01"

        result = build_date_gt_sql(path_sql, value)
        expected = "(data->>'start_date')::date > '2023-07-01'::date"

        assert result.as_string(None) == expected

    def test_build_date_greater_than_equal_sql(self) -> None:
        """Test Date greater than or equal operator."""
        path_sql = SQL("data->>'start_date'")
        value = "2023-07-01"

        result = build_date_gte_sql(path_sql, value)
        expected = "(data->>'start_date')::date >= '2023-07-01'::date"

        assert result.as_string(None) == expected

    def test_build_date_less_than_sql(self) -> None:
        """Test Date less than operator."""
        path_sql = SQL("data->>'end_date'")
        value = "2023-12-31"

        result = build_date_lt_sql(path_sql, value)
        expected = "(data->>'end_date')::date < '2023-12-31'::date"

        assert result.as_string(None) == expected

    def test_build_date_less_than_equal_sql(self) -> None:
        """Test Date less than or equal operator."""
        path_sql = SQL("data->>'end_date'")
        value = "2023-12-31"

        result = build_date_lte_sql(path_sql, value)
        expected = "(data->>'end_date')::date <= '2023-12-31'::date"

        assert result.as_string(None) == expected

    def test_date_range_queries(self) -> None:
        """Test Date range queries with comparison operators."""
        path_sql = SQL("data->>'event_date'")

        # Test month start
        result_month_start = build_date_gte_sql(path_sql, "2023-07-01")
        expected_month_start = "(data->>'event_date')::date >= '2023-07-01'::date"
        assert result_month_start.as_string(None) == expected_month_start

        # Test month end
        result_month_end = build_date_lte_sql(path_sql, "2023-07-31")
        expected_month_end = "(data->>'event_date')::date <= '2023-07-31'::date"
        assert result_month_end.as_string(None) == expected_month_end

        # Test quarter start
        result_quarter = build_date_gte_sql(path_sql, "2023-07-01")
        expected_quarter = "(data->>'event_date')::date >= '2023-07-01'::date"
        assert result_quarter.as_string(None) == expected_quarter

        # Test year boundary
        result_year = build_date_lt_sql(path_sql, "2024-01-01")
        expected_year = "(data->>'event_date')::date < '2024-01-01'::date"
        assert result_year.as_string(None) == expected_year


class TestDateValidation:
    """Test Date operator validation and error handling."""

    def test_date_in_requires_list(self) -> None:
        """Test that Date 'in' operator requires a list."""
        path_sql = SQL("data->>'date'")

        with pytest.raises(TypeError, match="'in' operator requires a list"):
            build_date_in_sql(path_sql, "2023-07-15")  # type: ignore[arg-type]

    def test_date_notin_requires_list(self) -> None:
        """Test that Date 'notin' operator requires a list."""
        path_sql = SQL("data->>'date'")

        with pytest.raises(TypeError, match="'notin' operator requires a list"):
            build_date_notin_sql(path_sql, "2023-07-15")  # type: ignore[arg-type]

    def test_date_iso_formats_supported(self) -> None:
        """Test that various ISO 8601 date formats are supported."""
        path_sql = SQL("data->>'date'")

        # Test valid ISO 8601 date formats
        valid_dates = [
            "2023-07-15",  # Standard format
            "2023-01-01",  # New Year's Day
            "2023-12-31",  # New Year's Eve
            "2023-02-28",  # Non-leap year
            "2024-02-29",  # Leap year
            "2023-04-30",  # Month with 30 days
            "2023-06-15",  # Mid-year date
            "2023-09-22",  # Autumn equinox
            "2023-12-21",  # Winter solstice
        ]

        for date_str in valid_dates:
            result = build_date_eq_sql(path_sql, date_str)
            expected = f"(data->>'date')::date = '{date_str}'::date"
            assert result.as_string(None) == expected

    def test_date_seasonal_boundaries(self) -> None:
        """Test Date with seasonal and calendar boundaries."""
        path_sql = SQL("data->>'date'")

        # Test leap year day
        result_leap = build_date_eq_sql(path_sql, "2024-02-29")
        expected_leap = "(data->>'date')::date = '2024-02-29'::date"
        assert result_leap.as_string(None) == expected_leap

        # Test month boundaries
        result_jan31 = build_date_eq_sql(path_sql, "2023-01-31")
        expected_jan31 = "(data->>'date')::date = '2023-01-31'::date"
        assert result_jan31.as_string(None) == expected_jan31

        result_apr30 = build_date_eq_sql(path_sql, "2023-04-30")
        expected_apr30 = "(data->>'date')::date = '2023-04-30'::date"
        assert result_apr30.as_string(None) == expected_apr30

    def test_date_business_scenarios(self) -> None:
        """Test Date with common business scenarios."""
        path_sql = SQL("data->>'business_date'")

        # Test fiscal year boundaries (assuming April start)
        fiscal_start = "2023-04-01"
        result_fiscal_start = build_date_gte_sql(path_sql, fiscal_start)
        expected_fiscal_start = f"(data->>'business_date')::date >= '{fiscal_start}'::date"
        assert result_fiscal_start.as_string(None) == expected_fiscal_start

        fiscal_end = "2024-03-31"
        result_fiscal_end = build_date_lte_sql(path_sql, fiscal_end)
        expected_fiscal_end = f"(data->>'business_date')::date <= '{fiscal_end}'::date"
        assert result_fiscal_end.as_string(None) == expected_fiscal_end

    def test_date_historical_and_future(self) -> None:
        """Test Date with historical and future dates."""
        path_sql = SQL("data->>'date'")

        # Test historical date
        result_historical = build_date_eq_sql(path_sql, "2000-01-01")
        expected_historical = "(data->>'date')::date = '2000-01-01'::date"
        assert result_historical.as_string(None) == expected_historical

        # Test future date
        result_future = build_date_eq_sql(path_sql, "2030-12-31")
        expected_future = "(data->>'date')::date = '2030-12-31'::date"
        assert result_future.as_string(None) == expected_future
