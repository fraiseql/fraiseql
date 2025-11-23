#!/usr/bin/env python3
"""Clean Example: Nested Array Where Filtering in FraiseQL v0.7.10+

This example demonstrates the CLEAN registration-based approach for nested array
where filtering with comprehensive AND/OR/NOT logical operator support,
following PrintOptim Backend conventions.

Features demonstrated:
- Clean registration-based API (no verbose field definitions)
- Complete logical operator support (AND/OR/NOT)
- Complex nested filtering conditions
- All standard field operators (equals, contains, gte, isnull, etc.)
"""

import asyncio
import uuid

from fraiseql.fields import fraise_field
from fraiseql.nested_array_filters import (
    auto_nested_array_filters,
    nested_array_filterable,
    register_nested_array_filter,
)
from fraiseql.types import fraise_type


# Step 1: Define your types normally (no verbose fraise_field)
@fraise_type
class PrintServer:
    """A print server in a network configuration."""

    id: uuid.UUID
    hostname: str
    ip_address: str | None = None
    operating_system: str
    n_total_allocations: int = 0


# Step 2a: Clean approach - automatic detection
@auto_nested_array_filters  # Automatically enables filtering for all list[T] fields
@fraise_type(sql_source="tv_network_configuration", jsonb_column="data")
class NetworkConfiguration:
    """Network configuration with automatically filterable nested arrays."""

    id: uuid.UUID
    identifier: str
    name: str
    # Simple, clean field definition - filtering enabled automatically!
    print_servers: list[PrintServer] = fraise_field(default_factory=list)


# Step 2b: Selective approach - decorator for specific fields
@nested_array_filterable("print_servers")  # Only enable for specific fields
@fraise_type(sql_source="tv_other_network", jsonb_column="data")
class OtherNetworkConfig:
    """Network configuration with selectively filterable arrays."""

    id: uuid.UUID
    name: str
    print_servers: list[PrintServer] = fraise_field(default_factory=list)
    # This field won't have filtering unless explicitly registered
    other_servers: list[PrintServer] = fraise_field(default_factory=list)


# Step 2c: Manual registration approach (maximum control)
@fraise_type(sql_source="tv_manual_network", jsonb_column="data")
class ManualNetworkConfig:
    """Network configuration with manually registered filtering."""

    id: uuid.UUID
    name: str
    print_servers: list[PrintServer] = fraise_field(default_factory=list)


# Manual registration (can be done anywhere - in filters.py, etc.)
register_nested_array_filter(ManualNetworkConfig, "print_servers", PrintServer)


async def main():
    """Demonstrate the clean nested array where filtering approaches."""
    print("🚀 Clean FraiseQL Nested Array Where Filtering")
    print("=" * 50)

    # Create sample data (same as before)
    network_config = NetworkConfiguration(
        id=uuid.uuid4(),
        identifier="clean-network-01",
        name="Clean Network Configuration",
        print_servers=[
            PrintServer(
                id=uuid.uuid4(),
                hostname="prod-server-01",
                ip_address="192.168.1.10",
                operating_system="Windows Server",
                n_total_allocations=150,
            ),
            PrintServer(
                id=uuid.uuid4(),
                hostname="dev-server-01",
                ip_address="192.168.1.20",
                operating_system="Linux",
                n_total_allocations=25,
            ),
            PrintServer(
                id=uuid.uuid4(),
                hostname="prod-server-02",
                ip_address=None,  # Offline server
                operating_system="Windows Server",
                n_total_allocations=0,
            ),
        ],
    )

    print(f"📊 Sample Data: {len(network_config.print_servers)} print servers")
    for server in network_config.print_servers:
        print(
            f"  • {server.hostname} ({server.operating_system}) - "
            f"{server.n_total_allocations} allocations"
        )
    print()

    # Show the registry information
    from fraiseql.nested_array_filters import list_registered_filters

    print("🔧 Registered Filters:")
    filters = list_registered_filters()
    for parent_type, fields in filters.items():
        print(f"  {parent_type}:")
        for field_name, element_type in fields.items():
            print(f"    - {field_name}: {element_type}")
    print()

    # Demonstrate filtering (same functionality as before, cleaner setup)
    from fraiseql.core.nested_field_resolver import create_nested_array_field_resolver_with_where
    from fraiseql.sql.graphql_where_generator import create_graphql_where_input

    PrintServerWhereInput = create_graphql_where_input(PrintServer)
    resolver = create_nested_array_field_resolver_with_where("print_servers", list[PrintServer])

    # Example 1: Simple implicit AND (multiple fields = implicit AND)
    print("📋 Example 1: Production servers with IP addresses (implicit AND)")
    where_filter = PrintServerWhereInput()
    where_filter.hostname = {"contains": "prod"}
    where_filter.ip_address = {"isnull": False}

    result = await resolver(network_config, None, where=where_filter)
    print(f"   Results: {len(result)} servers")
    for server in result:
        print(f"     • {server.hostname} - {server.ip_address}")
    print()

    # Example 2: Explicit AND operator
    print("📋 Example 2: High-spec servers (explicit AND)")
    condition1 = PrintServerWhereInput()
    condition1.operating_system = {"equals": "Windows Server"}

    condition2 = PrintServerWhereInput()
    condition2.n_total_allocations = {"gte": 100}

    where_filter = PrintServerWhereInput()
    where_filter.AND = [condition1, condition2]

    result = await resolver(network_config, None, where=where_filter)
    print(f"   Results: {len(result)} servers")
    for server in result:
        print(f"     • {server.hostname} - {server.operating_system}, {server.n_total_allocations} allocations")
    print()

    # Example 3: OR operator
    print("📋 Example 3: Linux OR high-allocation servers (OR)")
    linux_condition = PrintServerWhereInput()
    linux_condition.operating_system = {"equals": "Linux"}

    high_allocation_condition = PrintServerWhereInput()
    high_allocation_condition.n_total_allocations = {"gte": 100}

    where_filter = PrintServerWhereInput()
    where_filter.OR = [linux_condition, high_allocation_condition]

    result = await resolver(network_config, None, where=where_filter)
    print(f"   Results: {len(result)} servers")
    for server in result:
        print(f"     • {server.hostname} - {server.operating_system}, {server.n_total_allocations} allocations")
    print()

    # Example 4: NOT operator
    print("📋 Example 4: Non-Windows servers (NOT)")
    not_condition = PrintServerWhereInput()
    not_condition.operating_system = {"equals": "Windows Server"}

    where_filter = PrintServerWhereInput()
    where_filter.NOT = not_condition

    result = await resolver(network_config, None, where=where_filter)
    print(f"   Results: {len(result)} servers")
    for server in result:
        print(f"     • {server.hostname} - {server.operating_system}")
    print()

    # Example 5: Complex nested logic
    print("📋 Example 5: Complex nested conditions")
    print("   Query: (Windows AND active) OR (Linux AND high-allocation)")

    # (Windows AND active)
    windows_active = PrintServerWhereInput()
    windows_active.AND = [
        PrintServerWhereInput(operating_system={"equals": "Windows Server"}),
        PrintServerWhereInput(ip_address={"isnull": False})  # Active = has IP
    ]

    # (Linux AND high-allocation)
    linux_high_alloc = PrintServerWhereInput()
    linux_high_alloc.AND = [
        PrintServerWhereInput(operating_system={"equals": "Linux"}),
        PrintServerWhereInput(n_total_allocations={"gte": 20})
    ]

    where_filter = PrintServerWhereInput()
    where_filter.OR = [windows_active, linux_high_alloc]

    result = await resolver(network_config, None, where=where_filter)
    print(f"   Results: {len(result)} servers")
    for server in result:
        active_status = "active" if server.ip_address else "offline"
        print(f"     • {server.hostname} - {server.operating_system}, {server.n_total_allocations} allocations ({active_status})")
    print()

    print("✨ GraphQL Schema Generated:")
    print("""
    type NetworkConfiguration {
      id: UUID!
      identifier: String!
      name: String!
      printServers(where: PrintServerWhereInput): [PrintServer!]!  # ← Clean with full logical operator support!
    }

    input PrintServerWhereInput {
      # Field operators
      hostname: StringWhereInput
      ipAddress: StringWhereInput
      operatingSystem: StringWhereInput
      nTotalAllocations: IntWhereInput

      # Logical operators
      AND: [PrintServerWhereInput!]  # All conditions must be true
      OR: [PrintServerWhereInput!]   # Any condition can be true
      NOT: PrintServerWhereInput     # Invert condition result
    }

    input StringWhereInput {
      equals: String
      not: String
      in: [String!]
      notIn: [String!]
      contains: String
      startsWith: String
      endsWith: String
      isnull: Boolean
    }

    input IntWhereInput {
      equals: Int
      not: Int
      in: [Int!]
      notIn: [Int!]
      lt: Int
      lte: Int
      gt: Int
      gte: Int
      isnull: Boolean
    }

    # No verbose fraise_field definitions needed!
    # Just use @auto_nested_array_filters or @nested_array_filterable
    # Full logical operator support included automatically!
    """)

    print("🎉 Complete nested array filtering with logical operators works!")
    print()
    print("💡 Supported Query Patterns:")
    print("   • Simple conditions: { field: { operator: value } }")
    print("   • Implicit AND: Multiple fields in same filter")
    print("   • Explicit AND: { AND: [condition1, condition2] }")
    print("   • OR conditions: { OR: [condition1, condition2] }")
    print("   • NOT conditions: { NOT: condition }")
    print("   • Complex nesting: Unlimited depth of logical operators")
    print("   • All field operators: equals, contains, gte, isnull, etc.")


if __name__ == "__main__":
    asyncio.run(main())
