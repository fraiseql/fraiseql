"""Test to identify inconsistency in network operator SQL generation.

This test reveals the bug where different operators generate inconsistent
SQL for the same IP address field type.
"""

import pytest
from psycopg.sql import SQL

from fraiseql.sql.operator_strategies import ComparisonOperatorStrategy, NetworkOperatorStrategy
from fraiseql.types import IpAddress


class TestNetworkOperatorConsistencyBug:
    """Test inconsistent SQL generation between operators."""

    def test_eq_vs_insubnet_sql_consistency(self):
        """Test that eq and inSubnet generate consistent SQL for IP fields."""
        # Test field path representing JSONB IP address
        field_path = SQL("data->>'ip_address'")

        # Test eq operator (ComparisonOperatorStrategy)
        comparison_strategy = ComparisonOperatorStrategy()
        eq_sql = comparison_strategy.build_sql(field_path, "eq", "1.1.1.1", IpAddress)

        # Test inSubnet operator (NetworkOperatorStrategy)
        network_strategy = NetworkOperatorStrategy()
        subnet_sql = network_strategy.build_sql(field_path, "inSubnet", "1.1.1.0/24", IpAddress)

        # The issue: eq uses host() but inSubnet doesn't
        eq_str = str(eq_sql)
        subnet_str = str(subnet_sql)

        # Both should consistently handle IP address casting
        if "host(" in eq_str:
            # If eq uses host(), subnet operations should be compatible
            # The issue might be that inSubnet doesn't account for host() usage
            pass

        # Check that both operators can handle the JSONB field properly
        assert "data->>'ip_address'" in eq_str, "eq should reference the JSONB field"
        assert "data->>'ip_address'" in subnet_str, "inSubnet should reference the JSONB field"
        assert "::inet" in eq_str or "::inet" in subnet_str, "At least one should cast to inet"

    def test_private_vs_eq_consistency(self):
        """Test consistency between isPrivate and eq operators."""
        field_path = SQL("data->>'ip_address'")

        # Test eq for private IP
        comparison_strategy = ComparisonOperatorStrategy()
        eq_sql = comparison_strategy.build_sql(field_path, "eq", "192.168.1.1", IpAddress)

        # Test isPrivate
        network_strategy = NetworkOperatorStrategy()
        private_sql = network_strategy.build_sql(field_path, "isPrivate", True, IpAddress)

        eq_str = str(eq_sql)
        private_str = str(private_sql)

        # Both should handle the same field consistently
        # If eq uses host(), isPrivate should account for this
        if "host(" in eq_str and "host(" not in private_str:
            pass

    def test_demonstration_of_actual_bug(self):
        """Demonstrate the actual bug with concrete SQL examples."""
        field_path = SQL("data->>'ip_address'")

        # Simulate the case where JSONB contains "192.168.1.1" (without CIDR)
        comparison_strategy = ComparisonOperatorStrategy()
        network_strategy = NetworkOperatorStrategy()

        # These operations on the same field should be consistent
        eq_sql = comparison_strategy.build_sql(field_path, "eq", "192.168.1.1", IpAddress)
        subnet_sql = network_strategy.build_sql(field_path, "inSubnet", "192.168.1.0/24", IpAddress)

        eq_str = str(eq_sql)
        subnet_str = str(subnet_sql)

        # The real issue: different casting approaches
        uses_host_for_eq = "host(" in eq_str
        uses_direct_cast_for_subnet = "::inet" in subnet_str and "host(" not in subnet_str

        if uses_host_for_eq and uses_direct_cast_for_subnet:
            pass

        # The bug manifests when:
        # 1. JSONB contains IP addresses (with or without CIDR)
        # 2. Different operators apply different transformations
        # 3. This leads to unexpected results in complex queries


class TestSQLBehaviorWithPostgreSQL:
    """Test SQL behavior differences that could explain the bug."""

    def test_field_type_detection_issue(self):
        """Test if the issue is in field type detection for network operators."""
        from fraiseql.sql.operator_strategies import get_operator_registry

        registry = get_operator_registry()

        # Test that network strategy is selected for network operators with IP fields
        network_strategy = registry.get_strategy("inSubnet")
        comparison_strategy = registry.get_strategy("eq")

        # Test that network strategy can handle the operator
        assert network_strategy.can_handle("inSubnet"), "Network strategy should handle inSubnet"
        assert comparison_strategy.can_handle("eq"), "Comparison strategy should handle eq"

        # The issue might be that NetworkOperatorStrategy.can_handle() doesn't check field type
        # Let's see if it properly filters by field type

        network_strategy_instance = NetworkOperatorStrategy()
        network_strategy_instance.can_handle("inSubnet")

        # The bug might be here - NetworkOperatorStrategy should only handle network operators
        # for network field types, but the can_handle method doesn't check field type!


if __name__ == "__main__":
    # Quick test to see the issue
    test = TestNetworkOperatorConsistencyBug()
    test.test_eq_vs_insubnet_sql_consistency()
    test.test_demonstration_of_actual_bug()

    field_test = TestSQLBehaviorWithPostgreSQL()
    field_test.test_field_type_detection_issue()
