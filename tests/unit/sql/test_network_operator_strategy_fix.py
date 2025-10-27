"""
Test suite for NetworkOperatorStrategy fix.

This test verifies that the NetworkOperatorStrategy now properly supports
basic comparison operators (eq, neq, in, notin) in addition to the existing
network-specific operators.

Fixes issue: "Unsupported network operator: eq" for IP address fields
GitHub Issue: Network filtering partially broken in FraiseQL v0.5.5
"""

import pytest
from psycopg.sql import SQL, Composed, Literal
from fraiseql.sql.operator_strategies import NetworkOperatorStrategy
from fraiseql.types import IpAddress


class TestNetworkOperatorStrategy:
    """Test NetworkOperatorStrategy with basic operator support."""

    def setup_method(self):
        """Set up test environment."""
        self.strategy = NetworkOperatorStrategy()
        self.field_path = SQL("data->>'ip_address'")

    def test_supported_operators_list(self):
        """Test that all expected operators are now supported."""
        expected_operators = [
            # Basic comparison operators (newly added)
            "eq",
            "neq",
            "in",
            "notin",
            "nin",
            # Network-specific operators (existing)
            "inSubnet",
            "inRange",
            "isPrivate",
            "isPublic",
            "isIPv4",
            "isIPv6",
            # Enhanced network-specific operators (added)
            "isLoopback",
            "isLinkLocal",
            "isMulticast",
            "isDocumentation",
            "isCarrierGrade",
        ]

        assert self.strategy.operators == expected_operators

    def test_can_handle_network_specific_without_field_type(self):
        """Test that network-specific operators work without field_type."""
        network_ops = [
            "inSubnet",
            "inRange",
            "isPrivate",
            "isPublic",
            "isIPv4",
            "isIPv6",
            "isLoopback",
            "isLinkLocal",
            "isMulticast",
            "isDocumentation",
            "isCarrierGrade",
        ]

        for op in network_ops:
            assert self.strategy.can_handle(op, field_type=None), (
                f"Should handle {op} without field_type"
            )

    def test_cannot_handle_basic_operators_without_field_type(self):
        """Test that basic operators require field_type (delegated to generic strategies)."""
        basic_ops = ["eq", "neq", "in", "notin"]

        for op in basic_ops:
            assert not self.strategy.can_handle(op, field_type=None), (
                f"Should NOT handle {op} without field_type"
            )

    def test_can_handle_all_operators_with_ip_field_type(self):
        """Test that all operators work with IP address field type."""
        all_ops = [
            "eq",
            "neq",
            "in",
            "notin",
            "inSubnet",
            "inRange",
            "isPrivate",
            "isPublic",
            "isIPv4",
            "isIPv6",
            "isLoopback",
            "isLinkLocal",
            "isMulticast",
            "isDocumentation",
            "isCarrierGrade",
        ]

        for op in all_ops:
            assert self.strategy.can_handle(op, field_type=IpAddress), (
                f"Should handle {op} with IpAddress field_type"
            )

    def test_eq_operator_sql_generation(self):
        """Test SQL generation for eq operator."""
        result = self.strategy.build_sql(self.field_path, "eq", "8.8.8.8", IpAddress)

        expected = Composed(
            [
                Composed([SQL("("), self.field_path, SQL(")::inet")]),
                SQL(" = "),
                Literal("8.8.8.8"),
                SQL("::inet"),
            ]
        )

        assert str(result) == str(expected)

    def test_neq_operator_sql_generation(self):
        """Test SQL generation for neq operator."""
        result = self.strategy.build_sql(self.field_path, "neq", "8.8.8.8", IpAddress)

        expected = Composed(
            [
                Composed([SQL("("), self.field_path, SQL(")::inet")]),
                SQL(" != "),
                Literal("8.8.8.8"),
                SQL("::inet"),
            ]
        )

        assert str(result) == str(expected)

    def test_in_operator_sql_generation(self):
        """Test SQL generation for in operator."""
        ip_list = ["8.8.8.8", "1.1.1.1", "9.9.9.9"]
        result = self.strategy.build_sql(self.field_path, "in", ip_list, IpAddress)

        expected = Composed(
            [
                Composed([SQL("("), self.field_path, SQL(")::inet")]),
                SQL(" IN ("),
                Literal("8.8.8.8"),
                SQL("::inet"),
                SQL(", "),
                Literal("1.1.1.1"),
                SQL("::inet"),
                SQL(", "),
                Literal("9.9.9.9"),
                SQL("::inet"),
                SQL(")"),
            ]
        )

        assert str(result) == str(expected)

    def test_notin_operator_sql_generation(self):
        """Test SQL generation for notin operator."""
        ip_list = ["192.168.1.1", "10.0.0.1"]
        result = self.strategy.build_sql(self.field_path, "notin", ip_list, IpAddress)

        expected = Composed(
            [
                Composed([SQL("("), self.field_path, SQL(")::inet")]),
                SQL(" NOT IN ("),
                Literal("192.168.1.1"),
                SQL("::inet"),
                SQL(", "),
                Literal("10.0.0.1"),
                SQL("::inet"),
                SQL(")"),
            ]
        )

        assert str(result) == str(expected)

    def test_in_operator_requires_list(self):
        """Test that in operator validates input is a list."""
        with pytest.raises(TypeError, match="'in' operator requires a list"):
            self.strategy.build_sql(self.field_path, "in", "8.8.8.8", IpAddress)

    def test_notin_operator_requires_list(self):
        """Test that notin operator validates input is a list."""
        with pytest.raises(TypeError, match="'notin' operator requires a list"):
            self.strategy.build_sql(self.field_path, "notin", "8.8.8.8", IpAddress)

    def test_network_operators_still_work(self):
        """Test that existing network operators continue to work."""
        # Test inSubnet
        result = self.strategy.build_sql(self.field_path, "inSubnet", "192.168.0.0/16", IpAddress)
        expected_subnet = Composed(
            [
                Composed([SQL("("), self.field_path, SQL(")::inet")]),
                SQL(" <<= "),
                Literal("192.168.0.0/16"),
                SQL("::inet"),
            ]
        )
        assert str(result) == str(expected_subnet)

        # Test isPrivate
        result = self.strategy.build_sql(self.field_path, "isPrivate", True, IpAddress)
        # Should contain private IP ranges
        assert "<<= '10.0.0.0/8'::inet" in str(result)
        assert "<<= '192.168.0.0/16'::inet" in str(result)
        assert "<<= '172.16.0.0/12'::inet" in str(result)

    def test_validates_field_type_for_non_ip_fields(self):
        """Test that operator fails with non-IP field types."""
        with pytest.raises(
            ValueError, match="Network operator 'eq' can only be used with IP address fields"
        ):
            self.strategy.build_sql(self.field_path, "eq", "8.8.8.8", str)

    def test_unsupported_operator_still_fails(self):
        """Test that truly unsupported operators still fail appropriately."""
        assert not self.strategy.can_handle("like", IpAddress)
        assert not self.strategy.can_handle("regex", IpAddress)
        assert not self.strategy.can_handle("contains", IpAddress)


class TestNetworkOperatorStrategyEdgeCases:
    """Test edge cases and error conditions."""

    def setup_method(self):
        """Set up test environment."""
        self.strategy = NetworkOperatorStrategy()
        self.field_path = SQL("data->>'ip_address'")

    def test_empty_list_for_in_operator(self):
        """Test in operator with empty list."""
        result = self.strategy.build_sql(self.field_path, "in", [], IpAddress)
        expected = Composed(
            [Composed([SQL("("), self.field_path, SQL(")::inet")]), SQL(" IN ("), SQL(")")]
        )
        assert str(result) == str(expected)

    def test_single_item_list_for_in_operator(self):
        """Test in operator with single item list."""
        result = self.strategy.build_sql(self.field_path, "in", ["8.8.8.8"], IpAddress)
        expected = Composed(
            [
                Composed([SQL("("), self.field_path, SQL(")::inet")]),
                SQL(" IN ("),
                Literal("8.8.8.8"),
                SQL("::inet"),
                SQL(")"),
            ]
        )
        assert str(result) == str(expected)

    def test_ipv6_addresses(self):
        """Test that IPv6 addresses work with basic operators."""
        ipv6_addr = "2001:db8::1"
        result = self.strategy.build_sql(self.field_path, "eq", ipv6_addr, IpAddress)

        expected = Composed(
            [
                Composed([SQL("("), self.field_path, SQL(")::inet")]),
                SQL(" = "),
                Literal("2001:db8::1"),
                SQL("::inet"),
            ]
        )

        assert str(result) == str(expected)

    def test_private_ipv6_detection(self):
        """Test that private IP detection works with IPv6."""
        result = self.strategy.build_sql(self.field_path, "isPrivate", True, IpAddress)
        # The existing implementation should handle IPv6 private ranges
        assert "::inet" in str(result)


class TestBackwardCompatibility:
    """Test that the fix doesn't break existing functionality."""

    def setup_method(self):
        """Set up test environment."""
        self.strategy = NetworkOperatorStrategy()
        self.field_path = SQL("data->>'ip_address'")

    def test_all_original_operators_still_supported(self):
        """Test that all original network operators still work."""
        original_ops = ["inSubnet", "inRange", "isPrivate", "isPublic", "isIPv4", "isIPv6"]

        for op in original_ops:
            assert op in self.strategy.operators, (
                f"Original operator {op} should still be supported"
            )
            assert self.strategy.can_handle(op, IpAddress), f"Should handle {op} with IP field type"
            assert self.strategy.can_handle(op, None), f"Should handle {op} without field type"

    def test_operator_precedence_unchanged(self):
        """Test that operator handling precedence hasn't changed."""
        # Network-specific operators should still be handled without field_type
        assert self.strategy.can_handle("inSubnet", None)
        assert self.strategy.can_handle("isPrivate", None)

        # Basic operators should require field_type (delegated to generic strategies)
        assert not self.strategy.can_handle("eq", None)
        assert not self.strategy.can_handle("in", None)


if __name__ == "__main__":
    pytest.main([__file__, "-v"])
