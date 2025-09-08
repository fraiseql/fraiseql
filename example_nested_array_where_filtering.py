#!/usr/bin/env python3
"""Example: Nested Array Where Filtering in FraiseQL v0.7.10+

This example demonstrates the new nested array where filtering functionality
that builds on FraiseQL v0.7.10's nested array resolution capabilities.

The feature allows GraphQL queries to filter nested array elements based on
their properties using WhereInput types.

Usage:
    python example_nested_array_where_filtering.py
"""

import asyncio
import uuid
from typing import List, Optional

from fraiseql.core.nested_field_resolver import create_nested_array_field_resolver_with_where
from fraiseql.fields import fraise_field
from fraiseql.sql.graphql_where_generator import create_graphql_where_input
from fraiseql.types import fraise_type


# Step 1: Define your nested array element type
@fraise_type
class PrintServer:
    """A print server in a network configuration."""

    id: uuid.UUID
    hostname: str
    ip_address: Optional[str] = None
    operating_system: str
    n_total_allocations: int = 0
    identifier: Optional[str] = None


# Step 2: Define the parent type with where filtering enabled
@fraise_type(sql_source="tv_network_configuration", jsonb_column="data")
class NetworkConfiguration:
    """Network configuration with filterable print servers."""

    id: uuid.UUID
    identifier: str
    name: str

    # This field supports where filtering on nested array elements
    print_servers: List[PrintServer] = fraise_field(
        default_factory=list,
        supports_where_filtering=True,  # Enable where parameter
        nested_where_type=PrintServer,  # Generate WhereInput from this type
        description="Print servers with optional where filtering",
    )


async def main():
    """Demonstrate the nested array where filtering functionality."""
    print("🚀 FraiseQL Nested Array Where Filtering Example")
    print("=" * 50)

    # Step 3: Create sample data
    network_config = NetworkConfiguration(
        id=uuid.uuid4(),
        identifier="corp-network-01",
        name="Corporate Network Configuration",
        print_servers=[
            PrintServer(
                id=uuid.uuid4(),
                hostname="prod-server-01",
                ip_address="192.168.1.10",
                operating_system="Windows Server",
                n_total_allocations=150,
                identifier="PROD-WS-01",
            ),
            PrintServer(
                id=uuid.uuid4(),
                hostname="dev-server-01",
                ip_address="192.168.1.20",
                operating_system="Linux",
                n_total_allocations=25,
                identifier="DEV-LINUX-01",
            ),
            PrintServer(
                id=uuid.uuid4(),
                hostname="prod-server-02",
                ip_address=None,  # Offline server
                operating_system="Windows Server",
                n_total_allocations=0,
                identifier="PROD-WS-02",
            ),
            PrintServer(
                id=uuid.uuid4(),
                hostname="test-server-01",
                ip_address="192.168.1.30",
                operating_system="macOS",
                n_total_allocations=5,
                identifier="TEST-MAC-01",
            ),
        ],
    )

    print(f"📊 Sample Data: {len(network_config.print_servers)} print servers")
    for server in network_config.print_servers:
        print(
            f"  • {server.hostname} ({server.operating_system}) - {server.n_total_allocations} allocations"
        )
    print()

    # Step 4: Create WhereInput type for filtering
    PrintServerWhereInput = create_graphql_where_input(PrintServer)
    print("🔧 Generated WhereInput type: PrintServerWhereInput")
    print(f"   Available fields: {list(PrintServerWhereInput.__gql_fields__.keys())}")
    print()

    # Step 5: Create the enhanced field resolver
    resolver = create_nested_array_field_resolver_with_where("print_servers", List[PrintServer])
    print("✅ Enhanced resolver created with where parameter support")
    print()

    # Example 1: No filtering - return all servers
    print("📋 Example 1: No filtering (all servers)")
    all_servers = await resolver(network_config, None)
    print(f"   Results: {len(all_servers)} servers")
    print()

    # Example 2: Filter by hostname pattern
    print("📋 Example 2: Filter by hostname containing 'prod'")
    where_prod = PrintServerWhereInput()
    where_prod.hostname = {"contains": "prod"}

    prod_servers = await resolver(network_config, None, where=where_prod)
    print(f"   Results: {len(prod_servers)} servers")
    for server in prod_servers:
        print(f"     • {server.hostname}")
    print()

    # Example 3: Filter by online status (has IP address)
    print("📋 Example 3: Filter for online servers (has IP address)")
    where_online = PrintServerWhereInput()
    where_online.ip_address = {"isnull": False}

    online_servers = await resolver(network_config, None, where=where_online)
    print(f"   Results: {len(online_servers)} servers")
    for server in online_servers:
        print(f"     • {server.hostname} - {server.ip_address}")
    print()

    # Example 4: Filter by allocation range
    print("📋 Example 4: Filter by allocation count >= 50")
    where_high_alloc = PrintServerWhereInput()
    where_high_alloc.n_total_allocations = {"gte": 50}

    high_alloc_servers = await resolver(network_config, None, where=where_high_alloc)
    print(f"   Results: {len(high_alloc_servers)} servers")
    for server in high_alloc_servers:
        print(f"     • {server.hostname} - {server.n_total_allocations} allocations")
    print()

    # Example 5: Filter by operating system choices
    print("📋 Example 5: Filter by operating system (Windows or Linux)")
    where_os = PrintServerWhereInput()
    where_os.operating_system = {"in_": ["Windows Server", "Linux"]}

    filtered_os_servers = await resolver(network_config, None, where=where_os)
    print(f"   Results: {len(filtered_os_servers)} servers")
    for server in filtered_os_servers:
        print(f"     • {server.hostname} - {server.operating_system}")
    print()

    # Example 6: Complex multi-field filtering
    print("📋 Example 6: Complex filtering (prod servers + online + allocations > 0)")
    where_complex = PrintServerWhereInput()
    where_complex.hostname = {"startswith": "prod"}
    where_complex.ip_address = {"isnull": False}
    where_complex.n_total_allocations = {"gt": 0}

    complex_servers = await resolver(network_config, None, where=where_complex)
    print(f"   Results: {len(complex_servers)} servers")
    for server in complex_servers:
        print(
            f"     • {server.hostname} - {server.ip_address} - {server.n_total_allocations} allocations"
        )
    print()

    # Step 6: Demonstrate GraphQL integration
    print("🎯 GraphQL Schema Integration")
    print("In your GraphQL schema, this field would appear as:")
    print("""
    type NetworkConfiguration {
      id: UUID!
      identifier: String!
      name: String!
      printServers(where: PrintServerWhereInput): [PrintServer!]!
    }

    input PrintServerWhereInput {
      id: UUIDFilter
      hostname: StringFilter
      ipAddress: StringFilter
      operatingSystem: StringFilter
      nTotalAllocations: IntFilter
      identifier: StringFilter
    }

    # Example GraphQL Query:
    query GetNetworkConfig($id: UUID!) {
      networkConfiguration(id: $id) {
        name
        printServers(where: {
          hostname: { contains: "prod" }
          ipAddress: { isNotNull: true }
          nTotalAllocations: { gte: 100 }
        }) {
          hostname
          ipAddress
          operatingSystem
          nTotalAllocations
        }
      }
    }
    """)

    print("✨ Feature successfully demonstrated!")
    print("🎉 Nested Array Where Filtering is working in FraiseQL v0.7.10+")


if __name__ == "__main__":
    asyncio.run(main())
