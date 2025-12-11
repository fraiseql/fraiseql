"""Comprehensive tests for email, hostname, and MAC address operator SQL building."""

import pytest
from psycopg.sql import SQL

from fraiseql.sql.where.operators.email import (
    build_email_eq_sql,
    build_email_in_sql,
    build_email_neq_sql,
    build_email_notin_sql,
)
from fraiseql.sql.where.operators.hostname import (
    build_hostname_eq_sql,
    build_hostname_in_sql,
    build_hostname_neq_sql,
    build_hostname_notin_sql,
)
from fraiseql.sql.where.operators.mac_address import (
    build_mac_eq_sql,
    build_mac_in_sql,
    build_mac_neq_sql,
    build_mac_notin_sql,
)


class TestEmailOperators:
    """Test email operator SQL building."""

    def test_email_eq(self):
        """Test email equality operator."""
        path_sql = SQL("data->>'email'")
        result = build_email_eq_sql(path_sql, "user@example.com")
        sql_str = result.as_string(None)
        assert "data->>'email' = 'user@example.com'" == sql_str

    def test_email_neq(self):
        """Test email inequality operator."""
        path_sql = SQL("data->>'email'")
        result = build_email_neq_sql(path_sql, "spam@example.com")
        sql_str = result.as_string(None)
        assert "data->>'email' != 'spam@example.com'" == sql_str

    def test_email_in(self):
        """Test email IN operator."""
        path_sql = SQL("data->>'email'")
        result = build_email_in_sql(path_sql, ["admin@example.com", "support@example.com"])
        sql_str = result.as_string(None)
        assert "data->>'email' IN ('admin@example.com', 'support@example.com')" == sql_str

    def test_email_notin(self):
        """Test email NOT IN operator."""
        path_sql = SQL("data->>'email'")
        result = build_email_notin_sql(path_sql, ["banned@spam.com", "blocked@spam.com"])
        sql_str = result.as_string(None)
        assert "data->>'email' NOT IN ('banned@spam.com', 'blocked@spam.com')" == sql_str

    def test_email_with_special_chars(self):
        """Test email with special characters."""
        path_sql = SQL("data->>'email'")
        result = build_email_eq_sql(path_sql, "user+tag@sub.domain.example.com")
        sql_str = result.as_string(None)
        assert "user+tag@sub.domain.example.com" in sql_str

    def test_email_with_numbers(self):
        """Test email with numbers."""
        path_sql = SQL("data->>'email'")
        result = build_email_eq_sql(path_sql, "user123@example456.com")
        sql_str = result.as_string(None)
        assert "user123@example456.com" in sql_str


class TestHostnameOperators:
    """Test hostname operator SQL building."""

    def test_hostname_eq(self):
        """Test hostname equality operator."""
        path_sql = SQL("data->>'hostname'")
        result = build_hostname_eq_sql(path_sql, "api.example.com")
        sql_str = result.as_string(None)
        assert "data->>'hostname' = 'api.example.com'" == sql_str

    def test_hostname_neq(self):
        """Test hostname inequality operator."""
        path_sql = SQL("data->>'hostname'")
        result = build_hostname_neq_sql(path_sql, "old-server.example.com")
        sql_str = result.as_string(None)
        assert "data->>'hostname' != 'old-server.example.com'" == sql_str

    def test_hostname_in(self):
        """Test hostname IN operator."""
        path_sql = SQL("data->>'server'")
        result = build_hostname_in_sql(
            path_sql, ["web1.example.com", "web2.example.com", "web3.example.com"]
        )
        sql_str = result.as_string(None)
        assert (
            "data->>'server' IN ('web1.example.com', 'web2.example.com', 'web3.example.com')"
            == sql_str
        )

    def test_hostname_notin(self):
        """Test hostname NOT IN operator."""
        path_sql = SQL("data->>'server'")
        result = build_hostname_notin_sql(path_sql, ["blacklist1.com", "blacklist2.com"])
        sql_str = result.as_string(None)
        assert "data->>'server' NOT IN ('blacklist1.com', 'blacklist2.com')" == sql_str

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


class TestMacAddressOperators:
    """Test MAC address operator SQL building."""

    def test_mac_eq(self):
        """Test MAC address equality operator."""
        path_sql = SQL("data->>'mac_address'")
        result = build_mac_eq_sql(path_sql, "00:11:22:33:44:55")
        sql_str = result.as_string(None)
        assert "(data->>'mac_address')::macaddr = '00:11:22:33:44:55'::macaddr" == sql_str

    def test_mac_neq(self):
        """Test MAC address inequality operator."""
        path_sql = SQL("data->>'mac_address'")
        result = build_mac_neq_sql(path_sql, "ff:ff:ff:ff:ff:ff")
        sql_str = result.as_string(None)
        assert "(data->>'mac_address')::macaddr != 'ff:ff:ff:ff:ff:ff'::macaddr" == sql_str

    def test_mac_in(self):
        """Test MAC address IN operator."""
        path_sql = SQL("data->>'device_mac'")
        result = build_mac_in_sql(
            path_sql, ["00:11:22:33:44:55", "aa:bb:cc:dd:ee:ff", "12:34:56:78:9a:bc"]
        )
        sql_str = result.as_string(None)
        expected = "(data->>'device_mac')::macaddr IN ('00:11:22:33:44:55'::macaddr, 'aa:bb:cc:dd:ee:ff'::macaddr, '12:34:56:78:9a:bc'::macaddr)"
        assert expected == sql_str

    def test_mac_notin(self):
        """Test MAC address NOT IN operator."""
        path_sql = SQL("data->>'mac_address'")
        result = build_mac_notin_sql(path_sql, ["00:00:00:00:00:00", "ff:ff:ff:ff:ff:ff"])
        sql_str = result.as_string(None)
        expected = "(data->>'mac_address')::macaddr NOT IN ('00:00:00:00:00:00'::macaddr, 'ff:ff:ff:ff:ff:ff'::macaddr)"
        assert expected == sql_str

    def test_mac_uppercase(self):
        """Test MAC address with uppercase letters."""
        path_sql = SQL("data->>'mac_address'")
        result = build_mac_eq_sql(path_sql, "AA:BB:CC:DD:EE:FF")
        sql_str = result.as_string(None)
        assert "AA:BB:CC:DD:EE:FF" in sql_str

    def test_mac_mixed_case(self):
        """Test MAC address with mixed case."""
        path_sql = SQL("data->>'mac_address'")
        result = build_mac_eq_sql(path_sql, "Aa:Bb:Cc:Dd:Ee:Ff")
        sql_str = result.as_string(None)
        assert "Aa:Bb:Cc:Dd:Ee:Ff" in sql_str

    def test_mac_broadcast(self):
        """Test broadcast MAC address."""
        path_sql = SQL("data->>'mac_address'")
        result = build_mac_eq_sql(path_sql, "ff:ff:ff:ff:ff:ff")
        sql_str = result.as_string(None)
        assert "ff:ff:ff:ff:ff:ff" in sql_str

    def test_mac_zero(self):
        """Test zero MAC address."""
        path_sql = SQL("data->>'mac_address'")
        result = build_mac_eq_sql(path_sql, "00:00:00:00:00:00")
        sql_str = result.as_string(None)
        assert "00:00:00:00:00:00" in sql_str
