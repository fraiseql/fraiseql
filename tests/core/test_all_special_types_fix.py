"""Comprehensive test for all special types JSONB casting fix.

This test validates that ALL special types (Network, LTree, DateRange, MacAddress)
now work correctly with eq/neq operators even when field_type information is missing.
"""

import pytest
from psycopg.sql import SQL

from fraiseql.sql.operator_strategies import get_operator_registry


@pytest.mark.core
class TestAllSpecialTypesFix:
    """Test that all special types work with eq operator without field_type."""

    def test_all_special_types_comprehensive_fix(self):
        """Test that all special types get proper casting without field_type."""
        registry = get_operator_registry()
        jsonb_path = SQL("(data ->> 'test_field')")

        # Comprehensive test cases for all special types
        test_cases = [
            # Network types (IP addresses) - Fixed: eq operators use direct ::inet casting
            ("IPv4 Public", "8.8.8.8", "::inet", None),
            ("IPv4 Private", "192.168.1.1", "::inet", None),
            ("IPv4 Localhost", "127.0.0.1", "::inet", None),
            ("IPv6 Short", "::1", "::inet", None),
            ("IPv6 Full", "2001:db8::1", "::inet", None),
            # MAC addresses (should be detected before IP addresses)
            ("MAC Colon", "00:11:22:33:44:55", "::macaddr", None),
            ("MAC Hyphen", "00-11-22-33-44-55", "::macaddr", None),
            ("MAC Upper", "AA:BB:CC:DD:EE:FF", "::macaddr", None),
            # LTree hierarchical paths
            ("LTree Simple", "top.middle", "::ltree", None),
            ("LTree Complex", "org.dept.team.user", "::ltree", None),
            ("LTree Underscore", "app_config.db_settings", "::ltree", None),
            # DateRange temporal ranges
            ("DateRange Inclusive", "[2024-01-01,2024-12-31]", "::daterange", None),
            ("DateRange Exclusive", "(2024-01-01,2024-12-31)", "::daterange", None),
            ("DateRange Mixed", "[2024-01-01,2024-12-31)", "::daterange", None),
            # Regular strings (should NOT get special casting)
            ("Regular Text", "hello world", None, None),
            ("Domain Name", "example.com", None, None),  # Has dot but not LTree pattern
            ("File Path", "/path/to/file", None, None),
        ]

        strategy = registry.get_strategy("eq", field_type=None)

        for test_name, test_value, expected_cast, extra_check in test_cases:
            result = strategy.build_sql(jsonb_path, "eq", test_value, field_type=None)
            sql_str = str(result)

            print(f"\n{test_name}: {test_value}")
            print(f"  SQL: {sql_str}")

            if expected_cast:
                # Should have the expected casting
                assert expected_cast in sql_str, (
                    f"{test_name} should have {expected_cast} casting: {sql_str}"
                )

                # Should NOT have other special castings
                other_casts = ["::inet", "::ltree", "::daterange", "::macaddr"]
                other_casts.remove(expected_cast)

                for other_cast in other_casts:
                    assert other_cast not in sql_str, (
                        f"{test_name} should not have {other_cast} casting: {sql_str}"
                    )

                # Check for extra requirements (like host() for IP addresses)
                if extra_check:
                    assert extra_check in sql_str, (
                        f"{test_name} should contain '{extra_check}': {sql_str}"
                    )

                print(f"  ✅ CORRECT: Has {expected_cast} casting")

            else:
                # Should NOT have any special casting
                special_casts = ["::inet", "::ltree", "::daterange", "::macaddr"]
                for cast in special_casts:
                    assert cast not in sql_str, (
                        f"{test_name} should not have {cast} casting: {sql_str}"
                    )

                print(f"  ✅ CORRECT: No special casting")

    def test_edge_cases_and_ambiguous_values(self):
        """Test edge cases that might be ambiguous between types."""
        registry = get_operator_registry()
        jsonb_path = SQL("(data ->> 'test_field')")
        strategy = registry.get_strategy("eq", field_type=None)

        edge_cases = [
            # Values that could be confused between types
            ("Short Text", "a.b", "::ltree"),  # Simple LTree
            ("IPv4-like Invalid", "256.1.1.1", None),  # Invalid IP, no casting
            ("MAC-like Invalid", "GG:HH:II:JJ:KK:LL", None),  # Invalid MAC, no casting
            ("Date-like Invalid", "[invalid-date]", None),  # Invalid DateRange, no casting
            ("Empty String", "", None),  # Empty string, no casting
            # Valid patterns that should be detected
            ("Minimal LTree", "a.b", "::ltree"),
            ("Valid MAC No Separators", "001122334455", "::macaddr"),
            ("IPv6 Localhost", "::1", "::inet"),
        ]

        for test_name, test_value, expected_cast in edge_cases:
            result = strategy.build_sql(jsonb_path, "eq", test_value, field_type=None)
            sql_str = str(result)

            print(f"\n{test_name}: '{test_value}'")
            print(f"  SQL: {sql_str}")

            if expected_cast:
                assert expected_cast in sql_str, (
                    f"{test_name} should have {expected_cast} casting: {sql_str}"
                )
                print(f"  ✅ DETECTED: {expected_cast}")
            else:
                special_casts = ["::inet", "::ltree", "::daterange", "::macaddr"]
                for cast in special_casts:
                    assert cast not in sql_str, (
                        f"{test_name} should not have {cast} casting: {sql_str}"
                    )
                print(f"  ✅ NOT DETECTED: No special casting (correct)")

    def test_list_values_for_in_operator(self):
        """Test that lists of special type values work with 'in' operator."""
        registry = get_operator_registry()
        jsonb_path = SQL("(data ->> 'test_field')")
        strategy = registry.get_strategy("in", field_type=None)

        # Test list of IP addresses
        ip_list = ["192.168.1.1", "10.0.0.1", "8.8.8.8"]
        result = strategy.build_sql(jsonb_path, "in", ip_list, field_type=None)
        sql_str = str(result)

        print(f"IP list 'in' operator: {sql_str}")
        assert "::inet" in sql_str, "List of IPs should get inet casting"
        assert " IN " in sql_str, "Should use IN operator"

        # Test list of MAC addresses
        mac_list = ["00:11:22:33:44:55", "AA:BB:CC:DD:EE:FF"]
        result = strategy.build_sql(jsonb_path, "in", mac_list, field_type=None)
        sql_str = str(result)

        print(f"MAC list 'in' operator: {sql_str}")
        assert "::macaddr" in sql_str, "List of MACs should get macaddr casting"

        # Test list of LTree paths
        ltree_list = ["top.middle", "org.dept.team"]
        result = strategy.build_sql(jsonb_path, "in", ltree_list, field_type=None)
        sql_str = str(result)

        print(f"LTree list 'in' operator: {sql_str}")
        assert "::ltree" in sql_str, "List of LTrees should get ltree casting"

    def test_backward_compatibility_with_field_type(self):
        """Test that the fix doesn't break existing behavior when field_type is provided."""
        from fraiseql.types import IpAddress, LTree, DateRange, MacAddress

        registry = get_operator_registry()
        jsonb_path = SQL("(data ->> 'test_field')")

        # Test each special type with explicit field_type
        type_tests = [
            (IpAddress, "8.8.8.8", "::inet"),
            (LTree, "top.middle.bottom", "::ltree"),
            (DateRange, "[2024-01-01,2024-12-31)", "::daterange"),
            (MacAddress, "00:11:22:33:44:55", "::macaddr"),
        ]

        for field_type, test_value, expected_cast in type_tests:
            strategy = registry.get_strategy("eq", field_type=field_type)
            result = strategy.build_sql(jsonb_path, "eq", test_value, field_type=field_type)
            sql_str = str(result)

            print(f"{field_type.__name__} with field_type: {sql_str}")
            assert expected_cast in sql_str, (
                f"Backward compatibility broken for {field_type.__name__}"
            )

    def test_production_parity_scenarios(self):
        """Test scenarios that directly address the production failures."""
        registry = get_operator_registry()
        jsonb_path = SQL("(data ->> 'ip_address')")

        # This reproduces the exact production failure scenario
        production_tests = [
            # DNS server IP equality - the main production failure
            ("DNS IP Equality", "eq", "8.8.8.8", "::inet"),
            ("Private IP Detection", "eq", "192.168.1.1", "::inet"),
            ("Public IP Detection", "eq", "1.1.1.1", "::inet"),
        ]

        for test_name, op, test_value, expected_cast in production_tests:
            strategy = registry.get_strategy(op, field_type=None)
            result = strategy.build_sql(jsonb_path, op, test_value, field_type=None)
            sql_str = str(result)

            print(f"{test_name}: {sql_str}")
            assert expected_cast in sql_str, f"Production fix failed for {test_name}: {sql_str}"

            # Should use proper INET casting for comparison, not text comparison
            # Note: Fixed behavior no longer uses host() for equality operators
            assert "::inet" in sql_str, f"Should use INET casting for IP comparison: {sql_str}"

            print(f"  ✅ PRODUCTION FIX VALIDATED")


if __name__ == "__main__":
    print("Testing comprehensive special types fix...")

    test_instance = TestAllSpecialTypesFix()

    print("\n1. Testing all special types comprehensive fix...")
    test_instance.test_all_special_types_comprehensive_fix()

    print("\n2. Testing edge cases...")
    test_instance.test_edge_cases_and_ambiguous_values()

    print("\nRun full tests with: pytest tests/core/test_all_special_types_fix.py -m core -v -s")
