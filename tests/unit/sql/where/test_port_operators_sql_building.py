"""Tests for Port operators SQL building functions.

These tests verify that Port operators generate correct PostgreSQL SQL
with proper validation for network port operations.
"""

import pytest
from psycopg.sql import SQL

# Import Port operator functions
from fraiseql.sql.where.operators.port import (
    build_port_eq_sql,
    build_port_neq_sql,
    build_port_in_sql,
    build_port_notin_sql,
    build_port_gt_sql,
    build_port_gte_sql,
    build_port_lt_sql,
    build_port_lte_sql,
)


class TestPortBasicOperators:
    """Test basic Port operators (eq, neq, in, notin)."""

    def test_build_port_equality_sql(self):
        """Test Port equality operator with proper integer handling."""
        path_sql = SQL("data->>'server_port'")
        value = 8080

        result = build_port_eq_sql(path_sql, value)
        expected = "(data->>'server_port')::integer = 8080"

        assert result.as_string(None) == expected

    def test_build_port_inequality_sql(self):
        """Test Port inequality operator with proper integer handling."""
        path_sql = SQL("data->>'server_port'")
        value = 22

        result = build_port_neq_sql(path_sql, value)
        expected = "(data->>'server_port')::integer != 22"

        assert result.as_string(None) == expected

    def test_build_port_in_list_sql(self):
        """Test Port IN list with multiple port values."""
        path_sql = SQL("data->>'server_port'")
        value = [80, 443, 8080]

        result = build_port_in_sql(path_sql, value)
        expected = "(data->>'server_port')::integer IN (80, 443, 8080)"

        assert result.as_string(None) == expected

    def test_build_port_not_in_list_sql(self):
        """Test Port NOT IN list with multiple port values."""
        path_sql = SQL("data->>'server_port'")
        value = [22, 23, 3389]

        result = build_port_notin_sql(path_sql, value)
        expected = "(data->>'server_port')::integer NOT IN (22, 23, 3389)"

        assert result.as_string(None) == expected

    def test_build_port_single_item_in_list(self):
        """Test Port IN list with single value."""
        path_sql = SQL("data->>'server_port'")
        value = [3306]

        result = build_port_in_sql(path_sql, value)
        expected = "(data->>'server_port')::integer IN (3306)"

        assert result.as_string(None) == expected

    def test_build_port_common_ports(self):
        """Test Port operators with common well-known ports."""
        path_sql = SQL("data->>'port'")

        # Test HTTP
        result_http = build_port_eq_sql(path_sql, 80)
        expected_http = "(data->>'port')::integer = 80"
        assert result_http.as_string(None) == expected_http

        # Test HTTPS
        result_https = build_port_eq_sql(path_sql, 443)
        expected_https = "(data->>'port')::integer = 443"
        assert result_https.as_string(None) == expected_https

        # Test SSH
        result_ssh = build_port_eq_sql(path_sql, 22)
        expected_ssh = "(data->>'port')::integer = 22"
        assert result_ssh.as_string(None) == expected_ssh

    def test_build_port_empty_list_handling(self):
        """Test Port operators handle empty lists gracefully."""
        path_sql = SQL("data->>'port'")
        value = []

        result_in = build_port_in_sql(path_sql, value)
        expected_in = "(data->>'port')::integer IN ()"
        assert result_in.as_string(None) == expected_in

        result_notin = build_port_notin_sql(path_sql, value)
        expected_notin = "(data->>'port')::integer NOT IN ()"
        assert result_notin.as_string(None) == expected_notin


class TestPortComparisonOperators:
    """Test Port comparison operators (gt, gte, lt, lte)."""

    def test_build_port_greater_than_sql(self):
        """Test Port greater than operator."""
        path_sql = SQL("data->>'port'")
        value = 1024

        result = build_port_gt_sql(path_sql, value)
        expected = "(data->>'port')::integer > 1024"

        assert result.as_string(None) == expected

    def test_build_port_greater_than_equal_sql(self):
        """Test Port greater than or equal operator."""
        path_sql = SQL("data->>'port'")
        value = 1024

        result = build_port_gte_sql(path_sql, value)
        expected = "(data->>'port')::integer >= 1024"

        assert result.as_string(None) == expected

    def test_build_port_less_than_sql(self):
        """Test Port less than operator."""
        path_sql = SQL("data->>'port'")
        value = 49152

        result = build_port_lt_sql(path_sql, value)
        expected = "(data->>'port')::integer < 49152"

        assert result.as_string(None) == expected

    def test_build_port_less_than_equal_sql(self):
        """Test Port less than or equal operator."""
        path_sql = SQL("data->>'port'")
        value = 49152

        result = build_port_lte_sql(path_sql, value)
        expected = "(data->>'port')::integer <= 49152"

        assert result.as_string(None) == expected

    def test_port_range_queries(self):
        """Test Port range queries with comparison operators."""
        path_sql = SQL("data->>'service_port'")

        # Test well-known ports (1-1023)
        result_wellknown = build_port_lt_sql(path_sql, 1024)
        expected_wellknown = "(data->>'service_port')::integer < 1024"
        assert result_wellknown.as_string(None) == expected_wellknown

        # Test registered ports (1024-49151)
        result_registered_min = build_port_gte_sql(path_sql, 1024)
        expected_registered_min = "(data->>'service_port')::integer >= 1024"
        assert result_registered_min.as_string(None) == expected_registered_min

        result_registered_max = build_port_lte_sql(path_sql, 49151)
        expected_registered_max = "(data->>'service_port')::integer <= 49151"
        assert result_registered_max.as_string(None) == expected_registered_max

        # Test dynamic/private ports (49152-65535)
        result_dynamic = build_port_gt_sql(path_sql, 49151)
        expected_dynamic = "(data->>'service_port')::integer > 49151"
        assert result_dynamic.as_string(None) == expected_dynamic


class TestPortValidation:
    """Test Port operator validation and error handling."""

    def test_port_in_requires_list(self):
        """Test that Port 'in' operator requires a list."""
        path_sql = SQL("data->>'port'")

        with pytest.raises(TypeError, match="'in' operator requires a list"):
            build_port_in_sql(path_sql, 8080)

    def test_port_notin_requires_list(self):
        """Test that Port 'notin' operator requires a list."""
        path_sql = SQL("data->>'port'")

        with pytest.raises(TypeError, match="'notin' operator requires a list"):
            build_port_notin_sql(path_sql, 8080)

    def test_port_boundary_values(self):
        """Test Port operators with boundary values."""
        path_sql = SQL("data->>'port'")

        # Test minimum valid port (1)
        result_min = build_port_eq_sql(path_sql, 1)
        expected_min = "(data->>'port')::integer = 1"
        assert result_min.as_string(None) == expected_min

        # Test maximum valid port (65535)
        result_max = build_port_eq_sql(path_sql, 65535)
        expected_max = "(data->>'port')::integer = 65535"
        assert result_max.as_string(None) == expected_max

    def test_port_common_service_ports(self):
        """Test Port operators with common service ports."""
        path_sql = SQL("data->>'port'")

        # Test common service ports
        common_ports = [
            20,  # FTP Data
            21,  # FTP Control
            22,  # SSH
            23,  # Telnet
            25,  # SMTP
            53,  # DNS
            80,  # HTTP
            110,  # POP3
            143,  # IMAP
            443,  # HTTPS
            993,  # IMAPS
            995,  # POP3S
            1433,  # SQL Server
            3306,  # MySQL
            5432,  # PostgreSQL
            6379,  # Redis
            8080,  # HTTP Alternative
            9200,  # Elasticsearch
        ]

        result = build_port_in_sql(path_sql, common_ports)
        expected = "(data->>'port')::integer IN (20, 21, 22, 23, 25, 53, 80, 110, 143, 443, 993, 995, 1433, 3306, 5432, 6379, 8080, 9200)"
        assert result.as_string(None) == expected

    def test_port_high_range_ports(self):
        """Test Port operators with high-range port numbers."""
        path_sql = SQL("data->>'port'")

        # Test high-range ports near the maximum
        high_ports = [60000, 61000, 62000, 63000, 64000, 65000, 65535]

        result = build_port_in_sql(path_sql, high_ports)
        expected = "(data->>'port')::integer IN (60000, 61000, 62000, 63000, 64000, 65000, 65535)"
        assert result.as_string(None) == expected
