"""Test field_type propagation through the WHERE clause generation pipeline.

This test investigates why network operations work in isolated tests but fail in
production. The issue may be that field_type information is not propagating
correctly from the dataclass definitions through to the operator strategies.
"""

import pytest
from dataclasses import dataclass
from typing import get_type_hints

from fraiseql.types import IpAddress, LTree, DateRange, MacAddress
from fraiseql.sql.where_generator import safe_create_where_type


@dataclass
class TestNetworkModel:
    """Test model with network field types that should propagate through WHERE generation."""

    id: str
    name: str
    ip_address: IpAddress
    secondary_ip: IpAddress | None = None


@dataclass
class TestSpecialTypesModel:
    """Test model with all special field types."""

    id: str
    ip_address: IpAddress
    path: LTree
    period: DateRange
    mac: MacAddress


@pytest.mark.core
class TestFieldTypePropagation:
    """Test that field_type information propagates correctly."""

    def test_type_hints_extraction(self):
        """Test that get_type_hints correctly extracts special types."""
        type_hints = get_type_hints(TestNetworkModel)

        print(f"Type hints for TestNetworkModel: {type_hints}")

        # Verify that special types are detected
        assert "ip_address" in type_hints
        assert type_hints["ip_address"] == IpAddress

        # Also test optional types
        if "secondary_ip" in type_hints:
            # Should handle Optional[IpAddress] correctly
            print(f"secondary_ip type: {type_hints['secondary_ip']}")

    def test_where_type_generation_preserves_field_types(self):
        """Test that WHERE type generation preserves field type information."""
        WhereType = safe_create_where_type(TestNetworkModel)

        # Create an instance to test
        where_instance = WhereType()
        assert hasattr(where_instance, "ip_address")

        # Set up a filter condition
        where_instance.ip_address = {"isPrivate": True}

        # Generate SQL - this is where field_type should be used
        sql_result = where_instance.to_sql()

        assert sql_result is not None
        sql_str = str(sql_result)

        print(f"Generated WHERE SQL: {sql_str}")

        # The critical test: field_type should cause proper inet casting
        assert "::inet" in sql_str, f"Field type not propagated - no inet casting in: {sql_str}"

    def test_all_special_types_field_propagation(self):
        """Test field_type propagation for all special types."""
        WhereType = safe_create_where_type(TestSpecialTypesModel)
        where_instance = WhereType()

        # Test each special type
        test_cases = [
            ("ip_address", {"isPrivate": True}, "::inet"),
            ("path", {"ancestor_of": "top.middle"}, "::ltree"),
            ("period", {"contains_date": "2024-06-15"}, "::daterange"),
            ("mac", {"eq": "00:11:22:33:44:55"}, "::macaddr"),
        ]

        for field_name, filter_dict, expected_cast in test_cases:
            # Set the filter
            setattr(where_instance, field_name, filter_dict)

            # Generate SQL
            sql_result = where_instance.to_sql()
            assert sql_result is not None

            sql_str = str(sql_result)
            print(f"Field {field_name} SQL: {sql_str}")

            # Verify proper casting
            assert expected_cast in sql_str, (
                f"Field {field_name} missing {expected_cast} casting: {sql_str}"
            )

            # Reset for next test
            setattr(where_instance, field_name, {})

    def test_field_type_none_fallback_behavior(self):
        """Test what happens when field_type is None (the potential production issue)."""
        from fraiseql.sql.where_generator import _make_filter_field_composed

        # Simulate the case where field_type is None (this might be the production issue)
        json_path = "data"

        # Test with field_type=None (simulating production failure)
        result_no_type = _make_filter_field_composed(
            "ip_address",
            {"isPrivate": True},
            json_path,
            field_type=None,  # This might be the production issue
        )

        if result_no_type:
            sql_no_type = str(result_no_type)
            print(f"No field_type SQL: {sql_no_type}")

            # This might fail because without field_type, the operator registry
            # might not select the right strategy
        else:
            print("No SQL generated when field_type=None")

        # Test with proper field_type (simulating test environment success)
        result_with_type = _make_filter_field_composed(
            "ip_address", {"isPrivate": True}, json_path, field_type=IpAddress
        )

        if result_with_type:
            sql_with_type = str(result_with_type)
            print(f"With field_type SQL: {sql_with_type}")

            # This should work correctly
            assert "::inet" in sql_with_type, "Should have inet casting with field_type"

    def test_operator_registry_without_field_type(self):
        """Test operator registry behavior when field_type is not provided."""
        from fraiseql.sql.operator_strategies import get_operator_registry

        registry = get_operator_registry()

        # Test network operators without field_type
        network_ops = ["isPrivate", "isPublic", "inSubnet"]

        for op in network_ops:
            try:
                # This might fail if NetworkOperatorStrategy requires field_type
                strategy = registry.get_strategy(op, field_type=None)
                print(f"Operator {op} without field_type: {strategy.__class__.__name__}")

                # If it doesn't fail, the strategy should still work correctly
                from psycopg.sql import SQL

                if op in ("isPrivate", "isPublic"):
                    result = strategy.build_sql(SQL("data->>'ip_address'"), op, True, None)
                elif op == "inSubnet":
                    result = strategy.build_sql(
                        SQL("data->>'ip_address'"), op, "192.168.0.0/16", None
                    )

                sql_str = str(result)
                print(f"  Generated SQL: {sql_str}")

                # Check if it still does proper casting without field_type
                has_casting = "::inet" in sql_str
                print(f"  Has inet casting: {has_casting}")

            except Exception as e:
                print(f"Operator {op} failed without field_type: {e}")
                # This might be the root cause - some operators might fail without field_type

    def test_comparison_strategy_field_type_handling(self):
        """Test how ComparisonOperatorStrategy handles IpAddress types."""
        from fraiseql.sql.operator_strategies import get_operator_registry
        from psycopg.sql import SQL

        registry = get_operator_registry()

        # Test eq operator with and without field_type
        print("Testing eq operator with IpAddress:")

        # With field_type (should work)
        strategy_with_type = registry.get_strategy("eq", IpAddress)
        result_with_type = strategy_with_type.build_sql(
            SQL("data->>'ip_address'"), "eq", "8.8.8.8", IpAddress
        )
        sql_with_type = str(result_with_type)
        print(f"  With field_type: {sql_with_type}")

        # Without field_type (might be the issue)
        strategy_no_type = registry.get_strategy("eq", None)
        result_no_type = strategy_no_type.build_sql(
            SQL("data->>'ip_address'"), "eq", "8.8.8.8", None
        )
        sql_no_type = str(result_no_type)
        print(f"  Without field_type: {sql_no_type}")

        # Compare the results
        print(
            f"  Same strategy selected: {strategy_with_type.__class__ == strategy_no_type.__class__}"
        )
        print(f"  With type has inet casting: {'::inet' in sql_with_type}")
        print(f"  Without type has inet casting: {'::inet' in sql_no_type}")

        # This comparison might reveal the production issue


@pytest.mark.core
class TestProductionScenarioSimulation:
    """Simulate production scenarios that might cause field_type to be lost."""

    def test_field_type_loss_scenarios(self):
        """Test scenarios where field_type might be lost in production."""

        # Scenario 1: Different code path in production vs test
        # This might happen if production uses a different initialization path

        # Scenario 2: Serialization/deserialization losing type information
        # This might happen if the WHERE input goes through JSON serialization

        # Scenario 3: Different Python typing behavior in different environments
        # This might happen with different Python versions or typing library versions

        type_hints = get_type_hints(TestNetworkModel)
        print(f"Type hints in current environment: {type_hints}")

        # Check if the type is what we expect
        ip_type = type_hints.get("ip_address")
        print(f"IP address type: {ip_type}")
        print(f"Is IpAddress: {ip_type is IpAddress}")
        print(f"IpAddress module: {getattr(IpAddress, '__module__', 'unknown')}")

        # This information might help identify production vs test differences

    def test_graphql_integration_field_type_propagation(self):
        """Test field_type propagation through GraphQL input generation."""
        # This would test the full pipeline from GraphQL input to SQL generation
        # but requires GraphQL setup which might be complex

        # For now, just test the WHERE type generation
        WhereType = safe_create_where_type(TestNetworkModel)

        # Simulate GraphQL input parsing
        graphql_input = {"ipAddress": {"isPrivate": True}}  # Note: GraphQL uses camelCase

        # This would need to be converted to the internal format
        # The conversion might lose field_type information

        where_instance = WhereType()
        where_instance.ip_address = {"isPrivate": True}  # Direct assignment

        sql_result = where_instance.to_sql()
        assert sql_result is not None

        sql_str = str(sql_result)
        print(f"GraphQL simulation SQL: {sql_str}")

        assert "::inet" in sql_str, "GraphQL integration should preserve field_type"


if __name__ == "__main__":
    print("Testing field_type propagation...")

    test_instance = TestFieldTypePropagation()

    print("\n1. Testing type hints extraction...")
    test_instance.test_type_hints_extraction()

    print("\n2. Testing WHERE type generation...")
    test_instance.test_where_type_generation_preserves_field_types()

    print("\n3. Testing operator registry without field_type...")
    test_instance.test_operator_registry_without_field_type()

    print("\n4. Testing comparison strategy field_type handling...")
    test_instance.test_comparison_strategy_field_type_handling()

    print("\nRun full tests with: pytest tests/core/test_field_type_propagation.py -m core -v -s")
