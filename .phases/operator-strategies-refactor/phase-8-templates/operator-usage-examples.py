#!/usr/bin/env python3
"""Operator Strategy Usage Examples

This script demonstrates how to use FraiseQL's operator strategies.

Run with: python docs/examples/operator-usage.py
"""

from ipaddress import IPv4Address

from psycopg.sql import Identifier

from fraiseql.sql.operators import get_default_registry

# Get registry
registry = get_default_registry()

print("=" * 60)
print("FraiseQL Operator Strategy Examples")
print("=" * 60)

# Example 1: String operators
print("\n1. String Operators")
print("-" * 40)

sql = registry.build_sql("contains", "test", Identifier("name"), field_type=str)
print(f"contains: {sql.as_string(None)}")

sql = registry.build_sql("startswith", "pre", Identifier("name"), field_type=str)
print(f"startswith: {sql.as_string(None)}")

sql = registry.build_sql("matches", "^[A-Z].*", Identifier("name"), field_type=str)
print(f"matches: {sql.as_string(None)}")

# Example 2: Numeric operators
print("\n2. Numeric Operators")
print("-" * 40)

sql = registry.build_sql("gt", 42, Identifier("age"), field_type=int)
print(f"gt: {sql.as_string(None)}")

sql = registry.build_sql("in", [1, 2, 3], Identifier("status"), field_type=int)
print(f"in: {sql.as_string(None)}")

# Example 3: Network operators
print("\n3. Network Operators")
print("-" * 40)

sql = registry.build_sql("isprivate", None, Identifier("ip"), field_type=IPv4Address)
print(f"isprivate: {sql.as_string(None)}")

sql = registry.build_sql("insubnet", "192.168.0.0/16", Identifier("ip"), field_type=IPv4Address)
print(f"insubnet: {sql.as_string(None)}")

# Example 4: Boolean operators
print("\n4. Boolean Operators")
print("-" * 40)

sql = registry.build_sql("eq", True, Identifier("active"), field_type=bool)
print(f"eq: {sql.as_string(None)}")

sql = registry.build_sql("isnull", True, Identifier("deleted_at"), field_type=str)
print(f"isnull: {sql.as_string(None)}")

print("\n" + "=" * 60)
print("All examples completed successfully!")
print("=" * 60)
