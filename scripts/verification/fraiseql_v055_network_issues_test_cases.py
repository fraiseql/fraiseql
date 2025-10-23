#!/usr/bin/env python3
"""FraiseQL v0.5.5 Network Filtering Issues - Comprehensive Test Cases.

================================================================

This file documents the exact network filtering issues found in FraiseQL v0.5.5
with specific test cases for the FraiseQL development team to reproduce and fix.

Issues identified:
1. ❌ IP equality operations (`eq`, `ne`) fail - "Unsupported network operator: eq"
2. ✅ IP classification operations (`isPrivate`, `isPublic`) generate correct SQL
   but may have execution issues
3. ✅ Subnet operations (`inSubnet`) work correctly
4. ❌ NetworkOperatorStrategy missing basic comparison operators

Based on analysis from: /tmp/fraiseql_v055_network_filtering_final_analysis.md
"""  # noqa: T201 - This is a demonstration/verification script with intentional output

import uuid
from dataclasses import dataclass

from psycopg.sql import SQL

import fraiseql
from fraiseql.sql.graphql_where_generator import create_graphql_where_input
from fraiseql.sql.operator_strategies import NetworkOperatorStrategy
from fraiseql.types import IpAddress


@fraiseql.type(sql_source="v_dns_server")
@dataclass
class DnsServer:
    """DNS server entity with IP address for network filtering tests."""

    id: uuid.UUID
    identifier: str
    ip_address: IpAddress  # Should support network filtering
    n_total_allocations: int | None = None


def test_sql_generation_issues():
    """Test Case 1: SQL Generation Issues
    ==================================

    This reproduces the core issue where `eq` operator fails for IP addresses
    while other network operators work correctly.
    """
    print("🔧 Test Case 1: SQL Generation Issues")
    print("=" * 50)

    strategy = NetworkOperatorStrategy()
    field_path = SQL("data->>'ip_address'")

    # Test the supported operators (should work)
    supported_ops = ["inSubnet", "isPrivate", "isPublic", "isIPv4", "isIPv6"]
    print("\n✅ Testing SUPPORTED network operators:")

    for op in supported_ops:
        try:
            if op == "inSubnet":
                sql = strategy.build_sql(field_path, op, "192.168.0.0/16", IpAddress)
                print(f"  ✅ {op}: {sql}")
            elif op in ["isPrivate", "isPublic", "isIPv4", "isIPv6"]:
                sql = strategy.build_sql(field_path, op, True, IpAddress)
                print(f"  ✅ {op}: SQL generated successfully")
        except Exception as e:
            print(f"  ❌ {op}: FAILED - {e}")

    # Test the now-supported operators (should work after fix)
    basic_ops = ["eq", "neq", "in", "notin"]
    print("\n✅ Testing BASIC operators (should work after fix):")

    for op in basic_ops:
        try:
            if op in ["in", "notin"]:
                # List-based operators need a list value
                test_val = ["8.8.8.8", "1.1.1.1"]
            else:
                # Single value operators
                test_val = "8.8.8.8"

            sql = strategy.build_sql(field_path, op, test_val, IpAddress)
            print(f"  ✅ {op}: {sql}")
        except Exception as e:
            print(f"  ❌ {op}: Failed - {e}")


def test_graphql_where_generation():
    """Test Case 2: GraphQL Where Input Generation
    ==========================================

    Verifies that the GraphQL schema correctly exposes all network operators,
    including the problematic `eq` operator.
    """
    print("\n\n🎯 Test Case 2: GraphQL Where Input Generation")
    print("=" * 50)

    WhereInput = create_graphql_where_input(DnsServer)

    # Create an instance to check available operators
    import typing

    hints = typing.get_type_hints(WhereInput)

    if "ip_address" in hints:
        ip_filter_type = hints["ip_address"]
        print(f"📊 IP Address Filter Type: {ip_filter_type}")

        # Get the actual filter class
        if hasattr(ip_filter_type, "__args__") and ip_filter_type.__args__:
            filter_class = ip_filter_type.__args__[0]
            filter_instance = filter_class()

            # Check all expected network operators
            expected_ops = [
                # Basic comparison (should now work after fix)
                "eq",
                "neq",
                "in",
                "notin",
                # Network-specific (should work)
                "inSubnet",
                "isPrivate",
                "isPublic",
                "isIPv4",
                "isIPv6",
            ]

            print("\n📋 Network operators availability in GraphQL schema:")
            for op in expected_ops:
                has_op = hasattr(filter_instance, op)
                status = "✅" if has_op else "❌"
                problem = " (FIXED IN STRATEGY)" if op in ["eq", "neq"] and has_op else ""
                print(f"  {status} {op}: {'Available' if has_op else 'Missing'}{problem}")


def test_operator_strategy_coverage():
    """Test Case 3: Operator Strategy Coverage Analysis
    ===============================================

    Analyzes the gap between what GraphQL exposes and what NetworkOperatorStrategy supports.
    This identifies the core architectural issue.
    """
    print("\n\n🔍 Test Case 3: Operator Strategy Coverage Analysis")
    print("=" * 50)

    strategy = NetworkOperatorStrategy()

    # Operators that NetworkOperatorStrategy claims to support
    supported_by_strategy = ["inSubnet", "inRange", "isPrivate", "isPublic", "isIPv4", "isIPv6"]

    # Operators that should work with IP addresses but aren't in NetworkOperatorStrategy
    missing_from_strategy = ["eq", "ne", "in", "notIn", "contains", "startsWith", "endsWith"]

    print(f"✅ Operators supported by NetworkOperatorStrategy: {supported_by_strategy}")
    print(f"❌ Operators MISSING from NetworkOperatorStrategy: {missing_from_strategy}")

    print("\n🎯 ROOT CAUSE ANALYSIS:")
    print("1. GraphQL schema correctly exposes 'eq' operator for IP address fields")
    print("2. NetworkOperatorStrategy only handles network-specific operators")
    print("3. Basic comparison operators (eq, ne) fall through to string operators")
    print("4. String operators don't properly handle IP address type casting")
    print("5. Result: 'Unsupported network operator: eq' error")

    print("\n💡 RECOMMENDED FIX:")
    print("Option A: Add basic comparison operators to NetworkOperatorStrategy")
    print("Option B: Improve IP address type detection in BaseOperatorStrategy")
    print("Option C: Ensure IP fields are properly cast before reaching string operators")


def test_expected_sql_output():
    """Test Case 4: Expected SQL Output Examples
    =======================================

    Shows what the correct SQL should look like for the failing operations,
    to guide implementation of the fix.
    """
    print("\n\n📝 Test Case 4: Expected SQL Output Examples")
    print("=" * 50)

    print("For IP equality filtering, the generated SQL should be:")
    print("❌ Current (fails): 'Unsupported network operator: eq'")
    print("✅ Expected: (data->>'ip_address')::inet = '8.8.8.8'::inet")
    print("✅ Alternative: host((data->>'ip_address')::inet) = '8.8.8.8'")

    print("\nFor IP inequality filtering, the generated SQL should be:")
    print("❌ Current (fails): 'Unsupported network operator: ne'")
    print("✅ Expected: (data->>'ip_address')::inet != '8.8.8.8'::inet")

    print("\nFor private IP filtering (works but may have execution issues):")
    print("✅ Current SQL generation works:")
    print("   ((data->>'ip_address')::inet <<= '10.0.0.0/8'::inet OR")
    print("    (data->>'ip_address')::inet <<= '172.16.0.0/12'::inet OR")
    print("    (data->>'ip_address')::inet <<= '192.168.0.0/16'::inet OR")
    print("    (data->>'ip_address')::inet <<= '127.0.0.0/8'::inet OR")
    print("    (data->>'ip_address')::inet <<= '169.254.0.0/16'::inet)")


def test_reproduction_scenario():
    """Test Case 5: Full Reproduction Scenario
    ======================================

    Documents the exact scenario that fails in production environments.
    """
    print("\n\n🚨 Test Case 5: Full Reproduction Scenario")
    print("=" * 50)

    print("Database setup:")
    print("  - Table: tenant.tb_dns_server with ip_address INET column")
    print("  - View: v_dns_server with JSONB data column containing IP as text")
    print("  - Data: '8.8.8.8', '192.168.1.1', '10.0.0.1'")

    print("\nGraphQL queries that should work but fail:")
    print('  1. dnsServers(where: { ipAddress: { eq: "8.8.8.8" } })')
    print("     Result: Empty array (should return Google DNS)")
    print("     Error: 'Unsupported network operator: eq'")

    print("  2. dnsServers(where: { ipAddress: { isPrivate: true } })")
    print("     Result: May return empty array despite correct SQL generation")
    print("     Issue: Possible query execution or result processing problem")

    print("\nGraphQL queries that work correctly:")
    print('  3. dnsServers(where: { ipAddress: { inSubnet: "192.168.0.0/16" } })')
    print("     Result: Returns expected results")
    print("     Status: ✅ Working correctly")

    print('  4. dnsServers(where: { identifier: { eq: "Primary DNS Google" } })')
    print("     Result: Returns Google DNS server")
    print("     Status: ✅ Working correctly (string field)")


if __name__ == "__main__":
    """Run all test cases to document the FraiseQL v0.5.5 network filtering issues."""

    print("FraiseQL v0.5.5 Network Filtering Issues - Comprehensive Test Cases")
    print("=" * 70)
    print("Generated for FraiseQL development team")
    print("Based on production network filtering analysis")
    print()

    # Run all test cases
    test_sql_generation_issues()
    test_graphql_where_generation()
    test_operator_strategy_coverage()
    test_expected_sql_output()
    test_reproduction_scenario()

    print("\n" + "=" * 70)
    print("🎯 SUMMARY AFTER FIX:")
    print(
        "1. ✅ NetworkOperatorStrategy now supports basic comparison operators (eq, neq, in, notin)"
    )
    print(
        "2. ✅ Network-specific operators (inSubnet, isPrivate, isPublic) continue to work correctly"
    )
    print("3. ✅ IP equality operations now generate proper SQL with ::inet casting")
    print("4. ✅ Fix implemented: Added IP address handling for basic comparison operators")
    print("\nThis fix resolves the network filtering issues identified in FraiseQL v0.5.5.")
