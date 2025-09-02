"""Test IP address SQL building for WHERE clauses (TDD Red Cycle).

These tests focus on the core IP filtering bug: generating proper PostgreSQL
inet casting for IP address comparisons.
"""

import pytest
from psycopg.sql import SQL, Literal
from fraiseql.sql.where.operators.network import (
    build_ip_eq_sql,
    build_ip_neq_sql,
    build_ip_in_sql,
    build_ip_notin_sql,
)


class TestIPAddressSQLBuilding:
    """Test IP address SQL building functionality."""

    def test_build_ip_equality_sql(self):
        """Should build proper inet casting for IP equality."""
        # Red cycle - this will fail initially
        path_sql = SQL("(data ->> 'ip_address')")
        result = build_ip_eq_sql(path_sql, "192.168.1.1")

        # Should generate: ((data ->> 'ip_address'))::inet = '192.168.1.1'::inet
        sql_str = result.as_string(None)
        assert "::inet = '192.168.1.1'::inet" in sql_str
        assert "data ->> 'ip_address'" in sql_str

        # Should NOT use host() function which was the bug
        assert "host(" not in sql_str

    def test_build_ip_inequality_sql(self):
        """Should build proper inet casting for IP inequality."""
        # Red cycle - this will fail initially
        path_sql = SQL("(data ->> 'server_ip')")
        result = build_ip_neq_sql(path_sql, "10.0.0.1")

        # Should generate: (data ->> 'server_ip')::inet != '10.0.0.1'::inet
        sql_str = result.as_string(None)
        assert "data ->> 'server_ip'" in sql_str
        assert "::inet != '10.0.0.1'::inet" in sql_str

    def test_build_ip_in_list_sql(self):
        """Should build proper inet casting for IP IN lists."""
        # Red cycle - this will fail initially
        path_sql = SQL("(data ->> 'gateway_ip')")
        ip_list = ["192.168.1.1", "10.0.0.1", "172.16.0.1"]
        result = build_ip_in_sql(path_sql, ip_list)

        # Should generate: (data ->> 'gateway_ip')::inet IN ('192.168.1.1'::inet, '10.0.0.1'::inet, '172.16.0.1'::inet)
        sql_str = result.as_string(None)
        assert "data ->> 'gateway_ip'" in sql_str
        assert "IN (" in sql_str
        assert "'192.168.1.1'::inet" in sql_str
        assert "'10.0.0.1'::inet" in sql_str
        assert "'172.16.0.1'::inet" in sql_str

    def test_build_ip_not_in_list_sql(self):
        """Should build proper inet casting for IP NOT IN lists."""
        # Red cycle - this will fail initially
        path_sql = SQL("(data ->> 'host_ip')")
        ip_list = ["8.8.8.8", "8.8.4.4"]
        result = build_ip_notin_sql(path_sql, ip_list)

        # Should generate: (data ->> 'host_ip')::inet NOT IN ('8.8.8.8'::inet, '8.8.4.4'::inet)
        sql_str = result.as_string(None)
        assert "data ->> 'host_ip'" in sql_str
        assert "NOT IN (" in sql_str
        assert "'8.8.8.8'::inet" in sql_str
        assert "'8.8.4.4'::inet" in sql_str

    def test_build_ip_ipv6_equality_sql(self):
        """Should build proper inet casting for IPv6 addresses."""
        # Red cycle - this will fail initially
        path_sql = SQL("(data ->> 'ipv6_address')")
        result = build_ip_eq_sql(path_sql, "2001:db8::1")

        # Should generate: (data ->> 'ipv6_address')::inet = '2001:db8::1'::inet
        sql_str = result.as_string(None)
        assert "data ->> 'ipv6_address'" in sql_str
        assert "::inet = '2001:db8::1'::inet" in sql_str

    def test_build_ip_cidr_equality_sql(self):
        """Should build proper inet casting for CIDR networks."""
        # Red cycle - this will fail initially
        path_sql = SQL("(data ->> 'network')")
        result = build_ip_eq_sql(path_sql, "192.168.1.0/24")

        # Should generate: (data ->> 'network')::inet = '192.168.1.0/24'::inet
        sql_str = result.as_string(None)
        assert "data ->> 'network'" in sql_str
        assert "::inet = '192.168.1.0/24'::inet" in sql_str


class TestIPAddressNetworkOperators:
    """Test network-specific IP operators like inSubnet, isPrivate, etc."""

    def test_build_in_subnet_sql(self):
        """Should build proper subnet containment operator."""
        # Red cycle - this will fail initially
        from fraiseql.sql.where.operators.network import build_in_subnet_sql

        path_sql = SQL("(data ->> 'ip_address')")
        result = build_in_subnet_sql(path_sql, "192.168.1.0/24")

        # Should generate: (data ->> 'ip_address')::inet <<= '192.168.1.0/24'::inet
        sql_str = result.as_string(None)
        assert "data ->> 'ip_address'" in sql_str
        assert "<<= '192.168.1.0/24'::inet" in sql_str

    def test_build_is_private_sql(self):
        """Should build proper private IP detection."""
        # Red cycle - this will fail initially
        from fraiseql.sql.where.operators.network import build_is_private_sql

        path_sql = SQL("(data ->> 'server_ip')")
        result = build_is_private_sql(path_sql, True)

        # Should check RFC 1918 private ranges
        sql_str = result.as_string(None)
        assert "10.0.0.0/8" in sql_str
        assert "172.16.0.0/12" in sql_str
        assert "192.168.0.0/16" in sql_str
        assert "<<=" in sql_str  # Subnet containment operator

    def test_build_is_public_sql(self):
        """Should build proper public IP detection (inverse of private)."""
        # Red cycle - this will fail initially
        from fraiseql.sql.where.operators.network import build_is_public_sql

        path_sql = SQL("(data ->> 'external_ip')")
        result = build_is_public_sql(path_sql, True)

        # Should be NOT (private ranges)
        sql_str = result.as_string(None)
        assert "NOT" in sql_str
        assert "10.0.0.0/8" in sql_str
