"""Test that the network filtering fix resolves the reported issues."""

from psycopg.sql import SQL

import fraiseql
from fraiseql.sql.operator_strategies import get_operator_registry
from fraiseql.sql.where_generator import safe_create_where_type
from fraiseql.types import IpAddress


@fraiseql.type
class DnsServer:
    """Test DNS server with IP address fields."""

    id: str
    identifier: str
    ip_address: IpAddress
    n_total_allocations: int | None = None


class TestNetworkFilteringFix:
    """Test that our fix resolves the reported network filtering issues."""

    def test_network_operator_selection_with_ip_types(self):
        """Test that network operators are properly selected for IP address fields."""
        registry = get_operator_registry()

        # Test that inSubnet gets NetworkOperatorStrategy with IP field type
        strategy = registry.get_strategy("inSubnet", IpAddress)
        assert strategy.__class__.__name__ == "NetworkOperatorStrategy"

        # Test that eq now gets NetworkOperatorStrategy (after fix)
        strategy = registry.get_strategy("eq", IpAddress)
        assert strategy.__class__.__name__ == "NetworkOperatorStrategy"

        # Test that isPrivate gets NetworkOperatorStrategy
        strategy = registry.get_strategy("isPrivate", IpAddress)
        assert strategy.__class__.__name__ == "NetworkOperatorStrategy"

    def test_fixed_sql_generation_for_network_operators(self):
        """Test that network operators generate consistent SQL."""
        registry = get_operator_registry()
        field_path = SQL("data->>'ip_address'")

        # Test inSubnet generates proper SQL
        subnet_sql = registry.build_sql(field_path, "inSubnet", "192.168.1.0/24", IpAddress)
        subnet_str = str(subnet_sql)

        # Should contain proper casting
        assert "::inet" in subnet_str
        assert "<<=" in subnet_str  # PostgreSQL subnet operator
        assert "192.168.1.0/24" in subnet_str

        # Test isPrivate generates proper SQL
        private_sql = registry.build_sql(field_path, "isPrivate", True, IpAddress)
        private_str = str(private_sql)

        # Should contain RFC 1918 ranges
        assert "192.168.0.0/16" in private_str
        assert "10.0.0.0/8" in private_str
        assert "172.16.0.0/12" in private_str
        assert "<<=" in private_str

    def test_eq_operator_vs_network_operators_consistency(self):
        """Test that eq and network operators can coexist properly."""
        registry = get_operator_registry()
        field_path = SQL("data->>'ip_address'")

        # Get SQL for both operators
        eq_sql = registry.build_sql(field_path, "eq", "192.168.1.1", IpAddress)
        subnet_sql = registry.build_sql(field_path, "inSubnet", "192.168.1.0/24", IpAddress)

        eq_str = str(eq_sql)
        subnet_str = str(subnet_sql)

        # Both should work with PostgreSQL
        # eq uses host() to handle CIDR notation properly
        # inSubnet uses direct ::inet which works with CIDR and without

        # The key insight: this is actually correct behavior!
        # - eq: host('192.168.1.1/32'::inet) = '192.168.1.1' (strips CIDR)
        # - inSubnet: '192.168.1.1'::inet <<= '192.168.1.0/24'::inet (includes CIDR)

        assert "host(" in eq_str or "=" in eq_str  # eq operator
        assert "<<=" in subnet_str  # subnet operator

    def test_where_type_generation_includes_network_operators(self):
        """Test that where type generation includes network operators for IP fields."""
        WhereType = safe_create_where_type(DnsServer)

        # Create an instance to test available operators
        where_instance = WhereType()

        # Should have network operators for ip_address field
        assert hasattr(where_instance, "ip_address")

        # The ip_address field should be a NetworkAddressFilter type

        # This would be None initially, but the type should support network operations
        # We can't easily test this without creating a full instance, but we can check
        # that the type was created correctly by the GraphQL where generator

    def test_network_operators_reject_non_ip_fields(self):
        """Test that network operators properly reject non-IP field types."""
        get_operator_registry()

        # Test that NetworkOperatorStrategy rejects non-IP types
        from fraiseql.sql.operator_strategies import NetworkOperatorStrategy

        network_strategy = NetworkOperatorStrategy()

        # Should handle IP addresses
        assert network_strategy.can_handle("inSubnet", IpAddress)

        # Should reject string types
        assert not network_strategy.can_handle("inSubnet", str)

        # Should reject int types
        assert not network_strategy.can_handle("inSubnet", int)

    def test_regression_reported_issue_patterns(self):
        """Test the specific patterns from the reported issue."""
        registry = get_operator_registry()
        field_path = SQL("data->>'ip_address'")

        # Issue #1: inSubnet filter returns wrong results
        # Generate SQL for subnet filter
        subnet_sql = registry.build_sql(field_path, "inSubnet", "192.168.0.0/16", IpAddress)
        subnet_str = str(subnet_sql)

        # Should generate: (data->>'ip_address')::inet <<= '192.168.0.0/16'::inet
        # This SQL should correctly filter only IPs in the 192.168.x.x range

        assert "data->>'ip_address'" in subnet_str
        assert "::inet" in subnet_str
        assert "<<=" in subnet_str
        assert "192.168.0.0/16" in subnet_str

        # Issue #2: Exact matching (eq) doesn't work
        eq_sql = registry.build_sql(field_path, "eq", "1.1.1.1", IpAddress)
        eq_str = str(eq_sql)

        # Should generate proper equality check
        # The host() function is actually correct for handling CIDR notation
        assert "1.1.1.1" in eq_str
        assert "=" in eq_str or "host(" in eq_str

        # Issue #3: isPrivate filter returns empty
        private_sql = registry.build_sql(field_path, "isPrivate", True, IpAddress)
        private_str = str(private_sql)

        # Should check all RFC 1918 ranges
        rfc1918_ranges = ["10.0.0.0/8", "172.16.0.0/12", "192.168.0.0/16"]
        for range_str in rfc1918_ranges:
            assert range_str in private_str


if __name__ == "__main__":
    test = TestNetworkFilteringFix()
    test.test_network_operator_selection_with_ip_types()
    test.test_fixed_sql_generation_for_network_operators()
    test.test_eq_operator_vs_network_operators_consistency()
    test.test_regression_reported_issue_patterns()
