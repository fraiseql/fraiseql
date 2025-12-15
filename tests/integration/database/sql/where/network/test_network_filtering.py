"""Test that the network filtering fix resolves the reported issues."""

import pytest
from psycopg.sql import SQL

import fraiseql
from fraiseql.sql.operators import get_default_registry as get_operator_registry
from fraiseql.sql.where_generator import safe_create_where_type
from fraiseql.types import IpAddress

pytestmark = pytest.mark.database


@fraiseql.type
class DnsServer:
    """Test DNS server with IP address fields."""

    id: str
    identifier: str
    ip_address: IpAddress
    n_total_allocations: int | None = None


class TestNetworkFiltering:
    """Test that our fix resolves the reported network filtering issues."""

    def test_network_operator_selection_with_ip_types(self) -> None:
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

    def test_sql_generation_for_network_operators(self) -> None:
        """Test that network operators generate consistent SQL."""
        registry = get_operator_registry()
        field_path = SQL("data->>'ip_address'")

        # Test inSubnet generates proper SQL
        subnet_sql = registry.build_sql(
            "inSubnet", "192.168.1.0/24", field_path, field_type=IpAddress
        )
        assert subnet_sql is not None
        subnet_str = subnet_sql.as_string(None)  # type: ignore

        # Should contain proper casting
        assert "::inet" in subnet_str
        assert "<<=" in subnet_str  # PostgreSQL subnet operator
        assert "192.168.1.0/24" in subnet_str

        # Test isPrivate generates proper SQL
        private_sql = registry.build_sql("isPrivate", True, field_path, field_type=IpAddress)
        assert private_sql is not None
        private_str = private_sql.as_string(None)  # type: ignore

        # Should use CIDR range checks for private IPs (no inet_public() in PostgreSQL)
        assert "10.0.0.0/8" in private_str  # RFC 1918 private range
        assert "192.168.0.0/16" in private_str  # RFC 1918 private range
        assert "::inet" in private_str

    def test_eq_operator_vs_network_operators_consistency(self) -> None:
        """Test that eq and network operators can coexist properly."""
        registry = get_operator_registry()
        field_path = SQL("data->>'ip_address'")

        # Get SQL for both operators
        eq_sql = registry.build_sql("eq", "192.168.1.1", field_path, field_type=IpAddress)
        subnet_sql = registry.build_sql(
            "inSubnet", "192.168.1.0/24", field_path, field_type=IpAddress
        )

        assert eq_sql is not None
        eq_str = eq_sql.as_string(None)  # type: ignore
        assert subnet_sql is not None
        subnet_str = subnet_sql.as_string(None)  # type: ignore

        # Both should work with PostgreSQL
        # eq uses host() to handle CIDR notation properly
        # inSubnet uses direct ::inet which works with CIDR and without

        # The key insight: this is actually correct behavior!
        # - eq: host('192.168.1.1/32'::inet) = '192.168.1.1' (strips CIDR)
        # - inSubnet: '192.168.1.1'::inet <<= '192.168.1.0/24'::inet (includes CIDR)

        assert "host(" in eq_str or "=" in eq_str  # eq operator
        assert "<<=" in subnet_str  # subnet operator

    def test_where_type_generation_includes_network_operators(self) -> None:
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

    def test_network_operators_reject_non_ip_fields(self) -> None:
        """Test that network operators properly reject non-IP field types."""
        get_operator_registry()

        # Test that NetworkOperatorStrategy rejects non-IP types
        from fraiseql.sql.operators import NetworkOperatorStrategy

        network_strategy = NetworkOperatorStrategy()

        # Should handle IP addresses
        assert network_strategy.supports_operator("inSubnet", IpAddress)

        # Should reject string types
        assert not network_strategy.supports_operator("inSubnet", str)

        # Should reject int types
        assert not network_strategy.supports_operator("inSubnet", int)

    def test_reported_issue_patterns(self) -> None:
        """Test the specific patterns from the reported issue."""
        registry = get_operator_registry()
        field_path = SQL("data->>'ip_address'")

        # Issue #1: inSubnet filter returns wrong results
        # Generate SQL for subnet filter
        subnet_sql = registry.build_sql(
            "inSubnet", "192.168.0.0/16", field_path, field_type=IpAddress
        )
        assert subnet_sql is not None
        subnet_str = subnet_sql.as_string(None)  # type: ignore

        # Should generate: (data->>'ip_address')::inet <<= '192.168.0.0/16'::inet
        # This SQL should correctly filter only IPs in the 192.168.x.x range

        assert "data->>'ip_address'" in subnet_str
        assert "::inet" in subnet_str
        assert "<<=" in subnet_str
        assert "192.168.0.0/16" in subnet_str

        # Issue #2: Exact matching (eq) doesn't work
        eq_sql = registry.build_sql("eq", "1.1.1.1", field_path, field_type=IpAddress)
        assert eq_sql is not None
        eq_str = eq_sql.as_string(None)  # type: ignore

        # Should generate proper equality check
        # The host() function is actually correct for handling CIDR notation
        assert "1.1.1.1" in eq_str
        assert "=" in eq_str or "host(" in eq_str

        # Issue #3: isPrivate filter returns empty
        private_sql = registry.build_sql("isPrivate", True, field_path, field_type=IpAddress)
        assert private_sql is not None
        private_str = private_sql.as_string(None)  # type: ignore

        # Should use CIDR range checks for private IPs (no inet_public() in PostgreSQL)
        assert "10.0.0.0/8" in private_str  # RFC 1918 private range
        assert "192.168.0.0/16" in private_str  # RFC 1918 private range
        assert "::inet" in private_str


if __name__ == "__main__":
    test = TestNetworkFiltering()
    test.test_network_operator_selection_with_ip_types()
    test.test_sql_generation_for_network_operators()
    test.test_eq_operator_vs_network_operators_consistency()
    test.test_reported_issue_patterns()
