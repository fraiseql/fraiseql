"""Comprehensive tests for hostname operator SQL building.

Consolidated from test_hostname_operators_sql_building.py and hostname parts of test_email_hostname_mac_complete.py.
"""

import pytest
from psycopg.sql import SQL

from fraiseql.sql.where.operators.hostname import (
    build_hostname_eq_sql,
    build_hostname_in_sql,
    build_hostname_neq_sql,
    build_hostname_notin_sql,
)


class TestHostnameBasicOperators:
    """Test basic Hostname operators (eq, neq, in, notin)."""

    def test_hostname_eq(self):
        """Test hostname equality operator."""
        path_sql = SQL("data->>'hostname'")
        result = build_hostname_eq_sql(path_sql, "api.example.com")
        sql_str = result.as_string(None)
        assert sql_str == "data->>'hostname' = 'api.example.com'"

    def test_hostname_neq(self):
        """Test hostname inequality operator."""
        path_sql = SQL("data->>'hostname'")
        result = build_hostname_neq_sql(path_sql, "old-server.example.com")
        sql_str = result.as_string(None)
        assert sql_str == "data->>'hostname' != 'old-server.example.com'"

    def test_hostname_in(self):
        """Test hostname IN operator."""
        path_sql = SQL("data->>'server'")
        result = build_hostname_in_sql(
            path_sql, ["web1.example.com", "web2.example.com", "web3.example.com"]
        )
        sql_str = result.as_string(None)
        assert (
            sql_str
            == "data->>'server' IN ('web1.example.com', 'web2.example.com', 'web3.example.com')"
        )

    def test_hostname_notin(self):
        """Test hostname NOT IN operator."""
        path_sql = SQL("data->>'server'")
        result = build_hostname_notin_sql(path_sql, ["blacklist1.com", "blacklist2.com"])
        sql_str = result.as_string(None)
        assert sql_str == "data->>'server' NOT IN ('blacklist1.com', 'blacklist2.com')"

    def test_build_hostname_equality_sql(self) -> None:
        """Test Hostname equality operator with proper text handling."""
        path_sql = SQL("data->>'server_hostname'")
        value = "api.example.com"

        result = build_hostname_eq_sql(path_sql, value)
        expected = "data->>'server_hostname' = 'api.example.com'"

        assert result.as_string(None) == expected

    def test_build_hostname_inequality_sql(self) -> None:
        """Test Hostname inequality operator with proper text handling."""
        path_sql = SQL("data->>'server_hostname'")
        value = "old.example.com"

        result = build_hostname_neq_sql(path_sql, value)
        expected = "data->>'server_hostname' != 'old.example.com'"

        assert result.as_string(None) == expected

    def test_build_hostname_in_list_sql(self) -> None:
        """Test Hostname IN list with multiple hostname values."""
        path_sql = SQL("data->>'server_hostname'")
        value = ["api.example.com", "web.example.com", "db.example.com"]

        result = build_hostname_in_sql(path_sql, value)
        expected = (
            "data->>'server_hostname' IN ('api.example.com', 'web.example.com', 'db.example.com')"
        )

        assert result.as_string(None) == expected

    def test_build_hostname_not_in_list_sql(self) -> None:
        """Test Hostname NOT IN list with multiple hostname values."""
        path_sql = SQL("data->>'server_hostname'")
        value = ["test.example.com", "staging.example.com"]

        result = build_hostname_notin_sql(path_sql, value)
        expected = "data->>'server_hostname' NOT IN ('test.example.com', 'staging.example.com')"

        assert result.as_string(None) == expected

    def test_build_hostname_single_item_in_list(self) -> None:
        """Test Hostname IN list with single value."""
        path_sql = SQL("data->>'server_hostname'")
        value = ["prod.example.com"]

        result = build_hostname_in_sql(path_sql, value)
        expected = "data->>'server_hostname' IN ('prod.example.com')"

        assert result.as_string(None) == expected

    def test_build_hostname_different_formats(self) -> None:
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

    def test_build_hostname_empty_list_handling(self) -> None:
        """Test Hostname operators handle empty lists gracefully."""
        path_sql = SQL("data->>'hostname'")
        value = []

        result_in = build_hostname_in_sql(path_sql, value)
        expected_in = "data->>'hostname' IN ()"
        assert result_in.as_string(None) == expected_in

        result_notin = build_hostname_notin_sql(path_sql, value)
        expected_notin = "data->>'hostname' NOT IN ()"
        assert result_notin.as_string(None) == expected_notin

    def test_build_hostname_case_handling(self) -> None:
        """Test Hostname operators handle case normalization."""
        path_sql = SQL("data->>'hostname'")

        # Hostnames should be case-insensitive (typically normalized to lowercase)
        result_upper = build_hostname_eq_sql(path_sql, "API.EXAMPLE.COM")
        expected_upper = "data->>'hostname' = 'API.EXAMPLE.COM'"
        assert result_upper.as_string(None) == expected_upper

        result_mixed = build_hostname_eq_sql(path_sql, "Web.Example.Com")
        expected_mixed = "data->>'hostname' = 'Web.Example.Com'"
        assert result_mixed.as_string(None) == expected_mixed


class TestHostnameSpecialCases:
    """Test hostname operators with special cases."""

    def test_hostname_with_subdomain(self):
        """Test hostname with multiple subdomains."""
        path_sql = SQL("data->>'hostname'")
        result = build_hostname_eq_sql(path_sql, "deep.sub.domain.example.com")
        sql_str = result.as_string(None)
        assert "deep.sub.domain.example.com" in sql_str

    def test_hostname_localhost(self):
        """Test localhost hostname."""
        path_sql = SQL("data->>'hostname'")
        result = build_hostname_eq_sql(path_sql, "localhost")
        sql_str = result.as_string(None)
        assert "localhost" in sql_str

    def test_hostname_with_hyphen(self):
        """Test hostname with hyphens."""
        path_sql = SQL("data->>'hostname'")
        result = build_hostname_eq_sql(path_sql, "my-api-server.example.com")
        sql_str = result.as_string(None)
        assert "my-api-server.example.com" in sql_str


class TestHostnameValidation:
    """Test Hostname operator validation and error handling."""

    def test_hostname_in_requires_list(self) -> None:
        """Test that Hostname 'in' operator requires a list."""
        path_sql = SQL("data->>'hostname'")

        with pytest.raises(TypeError, match="'in' operator requires a list"):
            build_hostname_in_sql(path_sql, "api.example.com")  # type: ignore[arg-type]

    def test_hostname_notin_requires_list(self) -> None:
        """Test that Hostname 'notin' operator requires a list."""
        path_sql = SQL("data->>'hostname'")

        with pytest.raises(TypeError, match="'notin' operator requires a list"):
            build_hostname_notin_sql(path_sql, "api.example.com")  # type: ignore[arg-type]

    def test_hostname_formats_supported(self) -> None:
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

    def test_hostname_special_cases(self) -> None:
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
