"""Tests for Hostname operators SQL building functions.

These tests verify that Hostname operators generate correct PostgreSQL SQL
with proper validation for DNS hostname operations.
"""

import pytest
from psycopg.sql import SQL

# Import Hostname operator functions
from fraiseql.sql.where.operators.hostname import (
    build_hostname_eq_sql,
    build_hostname_neq_sql,
    build_hostname_in_sql,
    build_hostname_notin_sql,
)


class TestHostnameBasicOperators:
    """Test basic Hostname operators (eq, neq, in, notin)."""

    def test_build_hostname_equality_sql(self):
        """Test Hostname equality operator with proper text handling."""
        path_sql = SQL("data->>'server_hostname'")
        value = "api.example.com"

        result = build_hostname_eq_sql(path_sql, value)
        expected = "data->>'server_hostname' = 'api.example.com'"

        assert result.as_string(None) == expected

    def test_build_hostname_inequality_sql(self):
        """Test Hostname inequality operator with proper text handling."""
        path_sql = SQL("data->>'server_hostname'")
        value = "old.example.com"

        result = build_hostname_neq_sql(path_sql, value)
        expected = "data->>'server_hostname' != 'old.example.com'"

        assert result.as_string(None) == expected

    def test_build_hostname_in_list_sql(self):
        """Test Hostname IN list with multiple hostname values."""
        path_sql = SQL("data->>'server_hostname'")
        value = ["api.example.com", "web.example.com", "db.example.com"]

        result = build_hostname_in_sql(path_sql, value)
        expected = (
            "data->>'server_hostname' IN ('api.example.com', 'web.example.com', 'db.example.com')"
        )

        assert result.as_string(None) == expected

    def test_build_hostname_not_in_list_sql(self):
        """Test Hostname NOT IN list with multiple hostname values."""
        path_sql = SQL("data->>'server_hostname'")
        value = ["test.example.com", "staging.example.com"]

        result = build_hostname_notin_sql(path_sql, value)
        expected = "data->>'server_hostname' NOT IN ('test.example.com', 'staging.example.com')"

        assert result.as_string(None) == expected

    def test_build_hostname_single_item_in_list(self):
        """Test Hostname IN list with single value."""
        path_sql = SQL("data->>'server_hostname'")
        value = ["prod.example.com"]

        result = build_hostname_in_sql(path_sql, value)
        expected = "data->>'server_hostname' IN ('prod.example.com')"

        assert result.as_string(None) == expected

    def test_build_hostname_different_formats(self):
        """Test Hostname operators with different hostname formats."""
        path_sql = SQL("data->>'hostname'")

        # Test simple hostname
        result_simple = build_hostname_eq_sql(path_sql, "server")
        expected_simple = "data->>'hostname' = 'server'"
        assert result_simple.as_string(None) == expected_simple

        # Test subdomain
        result_subdomain = build_hostname_eq_sql(path_sql, "api.service.example.com")
        expected_subdomain = "data->>'hostname' = 'api.service.example.com'"
        assert result_subdomain.as_string(None) == expected_subdomain

        # Test with numbers and hyphens
        result_complex = build_hostname_eq_sql(path_sql, "web-01.east-1.example.com")
        expected_complex = "data->>'hostname' = 'web-01.east-1.example.com'"
        assert result_complex.as_string(None) == expected_complex

    def test_build_hostname_empty_list_handling(self):
        """Test Hostname operators handle empty lists gracefully."""
        path_sql = SQL("data->>'hostname'")
        value = []

        result_in = build_hostname_in_sql(path_sql, value)
        expected_in = "data->>'hostname' IN ()"
        assert result_in.as_string(None) == expected_in

        result_notin = build_hostname_notin_sql(path_sql, value)
        expected_notin = "data->>'hostname' NOT IN ()"
        assert result_notin.as_string(None) == expected_notin

    def test_build_hostname_case_handling(self):
        """Test Hostname operators handle case normalization."""
        path_sql = SQL("data->>'hostname'")

        # Hostnames should be case-insensitive (typically normalized to lowercase)
        result_upper = build_hostname_eq_sql(path_sql, "API.EXAMPLE.COM")
        expected_upper = "data->>'hostname' = 'API.EXAMPLE.COM'"
        assert result_upper.as_string(None) == expected_upper

        result_mixed = build_hostname_eq_sql(path_sql, "Web.Example.Com")
        expected_mixed = "data->>'hostname' = 'Web.Example.Com'"
        assert result_mixed.as_string(None) == expected_mixed


class TestHostnameValidation:
    """Test Hostname operator validation and error handling."""

    def test_hostname_in_requires_list(self):
        """Test that Hostname 'in' operator requires a list."""
        path_sql = SQL("data->>'hostname'")

        with pytest.raises(TypeError, match="'in' operator requires a list"):
            build_hostname_in_sql(path_sql, "api.example.com")

    def test_hostname_notin_requires_list(self):
        """Test that Hostname 'notin' operator requires a list."""
        path_sql = SQL("data->>'hostname'")

        with pytest.raises(TypeError, match="'notin' operator requires a list"):
            build_hostname_notin_sql(path_sql, "api.example.com")

    def test_hostname_formats_supported(self):
        """Test that various valid hostname formats are supported."""
        path_sql = SQL("data->>'hostname'")

        # Test valid hostname formats (per RFC 1123)
        valid_hostnames = [
            "server",  # Single label
            "api.example.com",  # Standard FQDN
            "web-01.east-1.example.com",  # With hyphens and numbers
            "long-subdomain-name.example.co.uk",  # Multiple levels
            "db1.internal",  # Short TLD
            "service.123test.net",  # Numbers in domain
        ]

        for hostname in valid_hostnames:
            result = build_hostname_eq_sql(path_sql, hostname)
            expected = f"data->>'hostname' = '{hostname}'"
            assert result.as_string(None) == expected

    def test_hostname_special_cases(self):
        """Test Hostname with edge cases and special formats."""
        path_sql = SQL("data->>'hostname'")

        # Test maximum length hostname components
        long_label = "a" * 63  # Maximum label length
        long_hostname = f"{long_label}.example.com"

        result_long = build_hostname_eq_sql(path_sql, long_hostname)
        expected_long = f"data->>'hostname' = '{long_hostname}'"
        assert result_long.as_string(None) == expected_long

        # Test internationalized domain (if supported)
        result_intl = build_hostname_eq_sql(path_sql, "xn--e1afmkfd.xn--p1ai")
        expected_intl = "data->>'hostname' = 'xn--e1afmkfd.xn--p1ai'"
        assert result_intl.as_string(None) == expected_intl
