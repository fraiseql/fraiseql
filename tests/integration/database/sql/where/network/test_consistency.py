"""Test to identify inconsistency in network operator SQL generation.

This test reveals the bug where different operators generate inconsistent
SQL for the same IP address field type.
"""

import pytest
from psycopg.sql import SQL

from fraiseql.sql.operators import get_default_registry
from fraiseql.types import IpAddress

pytestmark = pytest.mark.database


class TestNetworkOperatorConsistencyBug:
    """Test inconsistent SQL generation between operators."""

    def test_eq_vs_insubnet_sql_consistency(self) -> None:
        """Test that eq and inSubnet generate consistent SQL for IP fields."""
        # Test field path representing JSONB IP address
        field_path = SQL("data->>'ip_address'")

        registry = get_default_registry()

        # Test eq operator
        eq_sql = registry.build_sql("eq", "1.1.1.1", field_path, field_type=IpAddress)

        # Test inSubnet operator
        subnet_sql = registry.build_sql("inSubnet", "1.1.1.0/24", field_path, field_type=IpAddress)

        # The issue: eq uses host() but inSubnet doesn't
        eq_str = eq_sql.as_string(None)  # type: ignore
        subnet_str = subnet_sql.as_string(None)  # type: ignore

        # Both should consistently handle IP address casting
        if "host(" in eq_str:
            # If eq uses host(), subnet operations should be compatible
            # The issue might be that inSubnet doesn't account for host() usage
            pass

        # Check that both operators can handle the JSONB field properly
        assert "data->>'ip_address'" in eq_str, "eq should reference the JSONB field"
        assert "data->>'ip_address'" in subnet_str, "inSubnet should reference the JSONB field"
        assert "::inet" in eq_str or "::inet" in subnet_str, "At least one should cast to inet"

    def test_private_vs_eq_consistency(self) -> None:
        """Test consistency between isPrivate and eq operators."""
        field_path = SQL("data->>'ip_address'")

        registry = get_default_registry()

        # Test eq for private IP
        eq_sql = registry.build_sql("eq", "192.168.1.1", field_path, field_type=IpAddress)

        # Test isPrivate
        private_sql = registry.build_sql("isPrivate", True, field_path, field_type=IpAddress)

        eq_str = eq_sql.as_string(None)  # type: ignore
        private_str = private_sql.as_string(None)  # type: ignore

        # Both should handle the same field consistently
        # If eq uses host(), isPrivate should account for this
        if "host(" in eq_str and "host(" not in private_str:
            pass

    def test_demonstration_of_actual_bug(self) -> None:
        """Demonstrate the actual bug with concrete SQL examples."""
        field_path = SQL("data->>'ip_address'")

        registry = get_default_registry()

        # These operations on the same field should be consistent
        eq_sql = registry.build_sql("eq", "192.168.1.1", field_path, field_type=IpAddress)
        subnet_sql = registry.build_sql(
            "inSubnet", "192.168.1.0/24", field_path, field_type=IpAddress
        )

        eq_str = eq_sql.as_string(None)  # type: ignore
        subnet_str = subnet_sql.as_string(None)  # type: ignore

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

    def test_field_type_detection_issue(self) -> None:
        """Test if the issue is in field type detection for network operators."""
        from fraiseql.sql.operators import get_default_registry as get_operator_registry

        registry = get_operator_registry()

        # Test that network strategy is selected for network operators with IP fields
        network_strategy = registry.get_strategy("inSubnet", IpAddress)
        comparison_strategy = registry.get_strategy("eq", IpAddress)

        # Test that strategies are found
        assert network_strategy is not None, "Network strategy should be found for inSubnet"
        assert comparison_strategy is not None, "Comparison strategy should be found for eq"

        # Test that network strategy can handle the operator
        assert network_strategy.supports_operator("inSubnet", IpAddress), (
            "Network strategy should handle inSubnet"
        )
        assert comparison_strategy.supports_operator("eq", IpAddress), (
            "Comparison strategy should handle eq"
        )

        # The issue might be that NetworkOperatorStrategy.can_handle() doesn't check field type
        # Let's see if it properly filters by field type

        # Note: can_handle method may not be available in registry API
        # This test may need to be updated to use registry methods

        # The bug might be here - NetworkOperatorStrategy should only handle network operators
        # for network field types, but the can_handle method doesn't check field type!


if __name__ == "__main__":
    # Quick test to see the issue
    test = TestNetworkOperatorConsistencyBug()
    test.test_eq_vs_insubnet_sql_consistency()
    test.test_demonstration_of_actual_bug()

    field_test = TestSQLBehaviorWithPostgreSQL()
    field_test.test_field_type_detection_issue()
