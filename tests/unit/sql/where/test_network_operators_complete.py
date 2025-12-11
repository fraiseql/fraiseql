"""Comprehensive tests for network operator SQL building."""

import pytest
from psycopg.sql import SQL

from fraiseql.sql.where.operators.network import (
    build_in_subnet_sql,
    build_ip_eq_sql,
    build_ip_in_sql,
    build_ip_neq_sql,
    build_ip_notin_sql,
    build_is_private_sql,
    build_is_public_sql,
)


class TestNetworkBasicOperators:
    """Test basic network comparison operators."""

    def test_eq_ipv4(self):
        """Test IPv4 equality."""
        path_sql = SQL("ip_address")
        result = build_ip_eq_sql(path_sql, "192.168.1.1")
        # Should contain inet casting
        assert "::inet" in str(result)
        assert "192.168.1.1" in str(result)

    def test_eq_ipv6(self):
        """Test IPv6 equality."""
        path_sql = SQL("ip_address")
        result = build_ip_eq_sql(path_sql, "2001:db8::1")
        assert "::inet" in str(result)
        assert "2001:db8::1" in str(result)

    def test_neq_network(self):
        """Test network inequality."""
        path_sql = SQL("network")
        result = build_ip_neq_sql(path_sql, "10.0.0.0/8")
        assert "::inet" in str(result)
        assert "!=" in str(result)

    def test_in_operator(self):
        """Test IP address IN list."""
        path_sql = SQL("ip_address")
        result = build_ip_in_sql(path_sql, ["192.168.1.1", "10.0.0.1"])
        assert "::inet" in str(result)
        assert "IN" in str(result)

    def test_notin_operator(self):
        """Test IP address NOT IN list."""
        path_sql = SQL("ip_address")
        result = build_ip_notin_sql(path_sql, ["192.168.1.1", "10.0.0.1"])
        assert "::inet" in str(result)
        assert "NOT IN" in str(result)


class TestNetworkPrivatePublic:
    """Test private/public IP detection."""

    def test_isprivate_operator(self):
        """Test isprivate operator for private IP ranges."""
        path_sql = SQL("ip_address")
        result = build_is_private_sql(path_sql, True)
        # Should check for private IP ranges
        result_str = str(result)
        assert "192.168." in result_str or "10." in result_str or "172." in result_str

    def test_ispublic_operator(self):
        """Test ispublic operator (not private)."""
        path_sql = SQL("ip_address")
        result = build_is_public_sql(path_sql, True)
        # Should be NOT isprivate
        assert "NOT" in str(result).upper()


class TestNetworkSubnet:
    """Test subnet operations."""

    def test_insubnet_ipv4(self):
        """Test if IP is in subnet (IPv4)."""
        path_sql = SQL("ip_address")
        result = build_in_subnet_sql(path_sql, "192.168.1.0/24")
        assert "<<=" in str(result)  # PostgreSQL subnet contains operator
        assert "192.168.1.0/24" in str(result)

    def test_insubnet_ipv6(self):
        """Test if IP is in subnet (IPv6)."""
        path_sql = SQL("ip_address")
        result = build_in_subnet_sql(path_sql, "2001:db8::/32")
        assert "<<=" in str(result)
        assert "2001:db8::/32" in str(result)


class TestNetworkEdgeCases:
    """Test edge cases for network operators."""

    def test_localhost_ipv4(self):
        """Test localhost handling."""
        path_sql = SQL("ip_address")
        result = build_ip_eq_sql(path_sql, "127.0.0.1")
        assert "127.0.0.1" in str(result)

    def test_localhost_ipv6(self):
        """Test IPv6 localhost."""
        path_sql = SQL("ip_address")
        result = build_ip_eq_sql(path_sql, "::1")
        assert "::1" in str(result)

    def test_broadcast_address(self):
        """Test broadcast address."""
        path_sql = SQL("ip_address")
        result = build_ip_eq_sql(path_sql, "255.255.255.255")
        assert "255.255.255.255" in str(result)
