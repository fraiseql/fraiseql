"""Comprehensive tests for date, datetime, and port operator SQL building."""

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
from fraiseql.sql.where.operators.port import (
    build_port_eq_sql,
    build_port_gt_sql,
    build_port_gte_sql,
    build_port_in_sql,
    build_port_lt_sql,
    build_port_lte_sql,
    build_port_neq_sql,
    build_port_notin_sql,
)


class TestDateOperators:
    """Test date operator SQL building."""

    def test_date_eq(self):
        """Test date equality operator."""
        path_sql = SQL("data->>'birth_date'")
        result = build_date_eq_sql(path_sql, "2023-07-15")
        sql_str = result.as_string(None)
        assert "(data->>'birth_date')::date = '2023-07-15'::date" == sql_str

    def test_date_neq(self):
        """Test date inequality operator."""
        path_sql = SQL("data->>'birth_date'")
        result = build_date_neq_sql(path_sql, "2023-07-15")
        sql_str = result.as_string(None)
        assert "(data->>'birth_date')::date != '2023-07-15'::date" == sql_str

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

    def test_date_gt(self):
        """Test date greater than operator."""
        path_sql = SQL("data->>'created_date'")
        result = build_date_gt_sql(path_sql, "2023-01-01")
        sql_str = result.as_string(None)
        assert "(data->>'created_date')::date > '2023-01-01'::date" == sql_str

    def test_date_gte(self):
        """Test date greater than or equal operator."""
        path_sql = SQL("data->>'start_date'")
        result = build_date_gte_sql(path_sql, "2023-06-01")
        sql_str = result.as_string(None)
        assert "(data->>'start_date')::date >= '2023-06-01'::date" == sql_str

    def test_date_lt(self):
        """Test date less than operator."""
        path_sql = SQL("data->>'expiry_date'")
        result = build_date_lt_sql(path_sql, "2024-12-31")
        sql_str = result.as_string(None)
        assert "(data->>'expiry_date')::date < '2024-12-31'::date" == sql_str

    def test_date_lte(self):
        """Test date less than or equal operator."""
        path_sql = SQL("data->>'deadline'")
        result = build_date_lte_sql(path_sql, "2023-12-31")
        sql_str = result.as_string(None)
        assert "(data->>'deadline')::date <= '2023-12-31'::date" == sql_str


class TestDateTimeOperators:
    """Test datetime operator SQL building."""

    def test_datetime_eq(self):
        """Test datetime equality operator."""
        path_sql = SQL("data->>'created_at'")
        result = build_datetime_eq_sql(path_sql, "2023-07-15T14:30:00Z")
        sql_str = result.as_string(None)
        assert "(data->>'created_at')::timestamptz = '2023-07-15T14:30:00Z'::timestamptz" == sql_str

    def test_datetime_neq(self):
        """Test datetime inequality operator."""
        path_sql = SQL("data->>'modified_at'")
        result = build_datetime_neq_sql(path_sql, "2023-07-15T14:30:00Z")
        sql_str = result.as_string(None)
        assert "(data->>'modified_at')::timestamptz != '2023-07-15T14:30:00Z'::timestamptz" == sql_str

    def test_datetime_in(self):
        """Test datetime IN operator."""
        path_sql = SQL("data->>'event_time'")
        result = build_datetime_in_sql(
            path_sql, ["2023-01-01T00:00:00Z", "2023-12-31T23:59:59Z"]
        )
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

    def test_datetime_gt(self):
        """Test datetime greater than operator."""
        path_sql = SQL("data->>'created_at'")
        result = build_datetime_gt_sql(path_sql, "2023-01-01T00:00:00Z")
        sql_str = result.as_string(None)
        assert "(data->>'created_at')::timestamptz > '2023-01-01T00:00:00Z'::timestamptz" == sql_str

    def test_datetime_gte(self):
        """Test datetime greater than or equal operator."""
        path_sql = SQL("data->>'start_time'")
        result = build_datetime_gte_sql(path_sql, "2023-06-01T12:00:00Z")
        sql_str = result.as_string(None)
        assert "(data->>'start_time')::timestamptz >= '2023-06-01T12:00:00Z'::timestamptz" == sql_str

    def test_datetime_lt(self):
        """Test datetime less than operator."""
        path_sql = SQL("data->>'expires_at'")
        result = build_datetime_lt_sql(path_sql, "2024-12-31T23:59:59Z")
        sql_str = result.as_string(None)
        assert "(data->>'expires_at')::timestamptz < '2024-12-31T23:59:59Z'::timestamptz" == sql_str

    def test_datetime_lte(self):
        """Test datetime less than or equal operator."""
        path_sql = SQL("data->>'deadline'")
        result = build_datetime_lte_sql(path_sql, "2023-12-31T23:59:59Z")
        sql_str = result.as_string(None)
        assert "(data->>'deadline')::timestamptz <= '2023-12-31T23:59:59Z'::timestamptz" == sql_str


class TestPortOperators:
    """Test port operator SQL building."""

    def test_port_eq(self):
        """Test port equality operator."""
        path_sql = SQL("data->>'port'")
        result = build_port_eq_sql(path_sql, 8080)
        sql_str = result.as_string(None)
        assert "(data->>'port')::integer = 8080" == sql_str

    def test_port_neq(self):
        """Test port inequality operator."""
        path_sql = SQL("data->>'port'")
        result = build_port_neq_sql(path_sql, 80)
        sql_str = result.as_string(None)
        assert "(data->>'port')::integer != 80" == sql_str

    def test_port_in(self):
        """Test port IN operator."""
        path_sql = SQL("data->>'service_port'")
        result = build_port_in_sql(path_sql, [80, 443, 8080])
        sql_str = result.as_string(None)
        assert "(data->>'service_port')::integer IN (80, 443, 8080)" == sql_str

    def test_port_notin(self):
        """Test port NOT IN operator."""
        path_sql = SQL("data->>'excluded_port'")
        result = build_port_notin_sql(path_sql, [22, 23, 3389])
        sql_str = result.as_string(None)
        assert "(data->>'excluded_port')::integer NOT IN (22, 23, 3389)" == sql_str

    def test_port_gt(self):
        """Test port greater than operator."""
        path_sql = SQL("data->>'port'")
        result = build_port_gt_sql(path_sql, 1024)
        sql_str = result.as_string(None)
        assert "(data->>'port')::integer > 1024" == sql_str

    def test_port_gte(self):
        """Test port greater than or equal operator."""
        path_sql = SQL("data->>'port'")
        result = build_port_gte_sql(path_sql, 1024)
        sql_str = result.as_string(None)
        assert "(data->>'port')::integer >= 1024" == sql_str

    def test_port_lt(self):
        """Test port less than operator."""
        path_sql = SQL("data->>'port'")
        result = build_port_lt_sql(path_sql, 65535)
        sql_str = result.as_string(None)
        assert "(data->>'port')::integer < 65535" == sql_str

    def test_port_lte(self):
        """Test port less than or equal operator."""
        path_sql = SQL("data->>'port'")
        result = build_port_lte_sql(path_sql, 65535)
        sql_str = result.as_string(None)
        assert "(data->>'port')::integer <= 65535" == sql_str

    def test_port_boundary_values(self):
        """Test port operators with boundary values."""
        path_sql = SQL("data->>'port'")

        # Min port (1)
        result = build_port_gte_sql(path_sql, 1)
        sql_str = result.as_string(None)
        assert "(data->>'port')::integer >= 1" == sql_str

        # Max port (65535)
        result = build_port_lte_sql(path_sql, 65535)
        sql_str = result.as_string(None)
        assert "(data->>'port')::integer <= 65535" == sql_str

        # Common privileged port
        result = build_port_eq_sql(path_sql, 443)
        sql_str = result.as_string(None)
        assert "(data->>'port')::integer = 443" == sql_str
