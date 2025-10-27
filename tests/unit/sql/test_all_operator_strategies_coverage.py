"""
Comprehensive test for all special operator strategies.

This test verifies that all advanced type operator strategies have proper
basic operator coverage after the NetworkOperatorStrategy fix.

Ensures no other advanced types are missing basic operators like
NetworkOperatorStrategy was.
"""

import pytest
from fraiseql.sql.operator_strategies import (
    NetworkOperatorStrategy,
    MacAddressOperatorStrategy,
    DateRangeOperatorStrategy,
    LTreeOperatorStrategy,
)
from fraiseql.types import IpAddress, MacAddress, DateRange, LTree


class TestAllOperatorStrategiesCoverage:
    """Test that all special operator strategies have proper basic operator coverage."""

    def test_all_strategies_have_basic_operators(self):
        """Test that all special operator strategies include basic operators."""

        # Define expected basic operators that all special strategies should have
        basic_operators = {"eq", "neq", "in", "notin"}

        strategies_and_types = [
            (NetworkOperatorStrategy(), IpAddress, "Network"),
            (MacAddressOperatorStrategy(), MacAddress, "MAC Address"),
            (DateRangeOperatorStrategy(), DateRange, "Date Range"),
            (LTreeOperatorStrategy(), LTree, "LTree"),
        ]

        for strategy, field_type, strategy_name in strategies_and_types:
            print(f"\n🔍 Testing {strategy_name} Strategy:")
            print(f"   Supported operators: {strategy.operators}")

            # Check that all basic operators are supported
            strategy_operators = set(strategy.operators)
            missing_basic = basic_operators - strategy_operators

            assert not missing_basic, (
                f"{strategy_name} Strategy missing basic operators: {missing_basic}. "
                f"Has: {strategy_operators}"
            )

            print(f"   ✅ Has all basic operators: {basic_operators}")

            # Test that strategy can handle basic operators with field type
            for op in basic_operators:
                can_handle = strategy.can_handle(op, field_type)
                assert can_handle, (
                    f"{strategy_name} Strategy should handle '{op}' with {field_type} field type"
                )

            print(f"   ✅ Can handle all basic operators with field type")

    def test_network_operator_strategy_specific(self):
        """Test NetworkOperatorStrategy specifically (the one that was fixed)."""
        strategy = NetworkOperatorStrategy()

        # Test that it now has all expected operators
        expected_operators = {
            # Basic operators (were missing, now added)
            "eq",
            "neq",
            "in",
            "notin",
            "nin",
            # Network-specific operators (were always there)
            "inSubnet",
            "inRange",
            "isPrivate",
            "isPublic",
            "isIPv4",
            "isIPv6",
            # Enhanced network-specific operators (newly added)
            "isLoopback",
            "isLinkLocal",
            "isMulticast",
            "isDocumentation",
            "isCarrierGrade",
        }

        strategy_operators = set(strategy.operators)
        assert strategy_operators == expected_operators, (
            f"NetworkOperatorStrategy operators mismatch. "
            f"Expected: {expected_operators}, Got: {strategy_operators}"
        )

    def test_mac_address_operator_strategy_specific(self):
        """Test MacAddressOperatorStrategy (should already be complete)."""
        strategy = MacAddressOperatorStrategy()

        # Test that it has basic operators plus MAC-specific ones
        expected_basic = {"eq", "neq", "in", "notin"}
        expected_mac_specific = {"contains", "startswith", "endswith"}

        strategy_operators = set(strategy.operators)

        # Should have all basic operators
        missing_basic = expected_basic - strategy_operators
        assert not missing_basic, f"MAC Address Strategy missing basic operators: {missing_basic}"

        # Should have MAC-specific operators
        missing_specific = expected_mac_specific - strategy_operators
        assert not missing_specific, (
            f"MAC Address Strategy missing MAC-specific operators: {missing_specific}"
        )

    def test_daterange_operator_strategy_specific(self):
        """Test DateRangeOperatorStrategy (should already be complete)."""
        strategy = DateRangeOperatorStrategy()

        # Test that it has basic operators plus range-specific ones
        expected_basic = {"eq", "neq", "in", "notin"}
        expected_range_specific = {
            "contains_date",
            "overlaps",
            "adjacent",
            "strictly_left",
            "strictly_right",
        }

        strategy_operators = set(strategy.operators)

        # Should have all basic operators
        missing_basic = expected_basic - strategy_operators
        assert not missing_basic, f"Date Range Strategy missing basic operators: {missing_basic}"

        # Should have range-specific operators
        missing_specific = expected_range_specific - strategy_operators
        assert not missing_specific, (
            f"Date Range Strategy missing range-specific operators: {missing_specific}"
        )

    def test_ltree_operator_strategy_specific(self):
        """Test LTreeOperatorStrategy (should already be complete)."""
        strategy = LTreeOperatorStrategy()

        # Test that it has basic operators plus tree-specific ones
        expected_basic = {"eq", "neq", "in", "notin"}
        expected_tree_specific = {
            "ancestor_of",
            "descendant_of",
            "matches_lquery",
            "matches_ltxtquery",
        }

        strategy_operators = set(strategy.operators)

        # Should have all basic operators
        missing_basic = expected_basic - strategy_operators
        assert not missing_basic, f"LTree Strategy missing basic operators: {missing_basic}"

        # Should have tree-specific operators
        missing_specific = expected_tree_specific - strategy_operators
        assert not missing_specific, (
            f"LTree Strategy missing tree-specific operators: {missing_specific}"
        )

    def test_operator_precedence_consistency(self):
        """Test that all strategies follow consistent precedence patterns."""
        strategies_and_types = [
            (NetworkOperatorStrategy(), IpAddress, "Network"),
            (MacAddressOperatorStrategy(), MacAddress, "MAC Address"),
            (DateRangeOperatorStrategy(), DateRange, "Date Range"),
            (LTreeOperatorStrategy(), LTree, "LTree"),
        ]

        for strategy, field_type, strategy_name in strategies_and_types:
            print(f"\n🔍 Testing {strategy_name} Strategy precedence:")

            # With field_type=None, should handle type-specific operators
            # but may not handle basic operators (delegated to generic strategies)

            # With proper field_type, should handle ALL operators including basic ones
            basic_operators = ["eq", "neq", "in", "notin"]
            for op in basic_operators:
                can_handle_with_type = strategy.can_handle(op, field_type)
                assert can_handle_with_type, (
                    f"{strategy_name} Strategy should handle '{op}' with {field_type} field type"
                )

            print(f"   ✅ Handles all basic operators with field type")

    def test_backward_compatibility(self):
        """Test that all strategies maintain backward compatibility."""
        strategies_and_types = [
            (NetworkOperatorStrategy(), IpAddress, "Network"),
            (MacAddressOperatorStrategy(), MacAddress, "MAC Address"),
            (DateRangeOperatorStrategy(), DateRange, "Date Range"),
            (LTreeOperatorStrategy(), LTree, "LTree"),
        ]

        for strategy, field_type, strategy_name in strategies_and_types:
            print(f"\n🔍 Testing {strategy_name} Strategy backward compatibility:")

            # All strategies should support their type-specific operators
            # These are examples of type-specific operators for each strategy
            type_specific_ops = {
                "Network": ["inSubnet", "isPrivate", "isPublic"],
                "MAC Address": ["contains", "startswith", "endswith"],
                "Date Range": ["contains_date", "overlaps", "adjacent"],
                "LTree": ["ancestor_of", "descendant_of", "matches_lquery"],
            }

            for op in type_specific_ops[strategy_name]:
                if op in strategy.operators:
                    can_handle = strategy.can_handle(op, field_type)
                    assert can_handle, (
                        f"{strategy_name} Strategy should handle type-specific '{op}'"
                    )

            print(f"   ✅ Maintains backward compatibility for type-specific operators")


if __name__ == "__main__":
    pytest.main([__file__, "-v"])
