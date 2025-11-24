"""Nested Object Filtering Example

This example demonstrates FraiseQL's nested object filtering capabilities
introduced in v0.11.6. You can filter on nested JSONB objects using both
dictionary-based and GraphQL WhereInput approaches.
"""

import asyncio
import uuid
from datetime import datetime

import fraiseql
from fraiseql.sql import create_graphql_where_input


@fraiseql.type
class Machine:
    """Machine type for nested filtering examples."""

    id: uuid.UUID
    name: str
    type: str
    power_watts: int


@fraiseql.type
class Location:
    """Location type with nested address."""

    id: uuid.UUID
    name: str
    address: dict  # Nested object in JSONB


@fraiseql.type(sql_source="tv_allocation")
class Allocation:
    """Allocation with nested machine and location objects."""

    id: uuid.UUID
    machine: Machine | None
    location: Location | None
    status: str
    created_at: datetime


# Example 1: Dictionary-based nested filtering
@fraiseql.query
async def allocations_by_machine_name(info, machine_name: str) -> list[Allocation]:
    """Find allocations by machine name using nested filtering."""
    repo = info.context["repo"]

    # Filter on nested machine.name field
    where = {"machine": {"name": {"eq": machine_name}}}

    return await repo.find("tv_allocation", where=where)


# Example 2: Multiple nested conditions
@fraiseql.query
async def active_server_allocations(info, min_power: int = 100) -> list[Allocation]:
    """Find active server allocations with minimum power requirements."""
    repo = info.context["repo"]

    where = {
        "status": {"eq": "active"},
        "machine": {"type": {"eq": "Server"}, "power_watts": {"gte": min_power}},
    }

    return await repo.find("tv_allocation", where=where)


# Example 3: Deep nesting (location.address.city)
@fraiseql.query
async def allocations_in_city(info, city: str) -> list[Allocation]:
    """Find allocations in a specific city."""
    repo = info.context["repo"]

    where = {"location": {"address": {"city": {"eq": city}}}}

    return await repo.find("tv_allocation", where=where)


# Example 4: GraphQL WhereInput approach (type-safe)
@fraiseql.query
async def allocations_with_filters(
    info,
    machine_name: str | None = None,
    machine_type: str | None = None,
    status: str | None = None,
) -> list[Allocation]:
    """Find allocations using GraphQL WhereInput objects for type safety."""
    repo = info.context["repo"]

    # Create WhereInput types
    MachineWhereInput = create_graphql_where_input(Machine)
    AllocationWhereInput = create_graphql_where_input(Allocation)

    # Build nested where clause
    machine_filters = {}
    if machine_name:
        machine_filters["name"] = {"eq": machine_name}
    if machine_type:
        machine_filters["type"] = {"eq": machine_type}

    where = AllocationWhereInput(
        machine=MachineWhereInput(**machine_filters) if machine_filters else None,
        status={"eq": status} if status else None,
    )

    return await repo.find("tv_allocation", where=where)


# Example 5: Complex nested filtering with multiple operators
@fraiseql.query
async def high_power_allocations_in_datacenter(info, datacenter: str) -> list[Allocation]:
    """Find high-power allocations in a specific datacenter."""
    repo = info.context["repo"]

    where = {
        "machine": {"power_watts": {"gte": 500}, "type": {"in": ["Server", "Storage"]}},
        "location": {"name": {"eq": datacenter}, "address": {"type": {"eq": "datacenter"}}},
        "status": {"neq": "decommissioned"},
    }

    return await repo.find("tv_allocation", where=where)


# GraphQL Schema Usage Examples:
#
# Query allocations by machine name
# query GetAllocationsByMachine {
#   allocationsByMachineName(machineName: "Server-01") {
#     id
#     status
#     machine {
#       name
#       type
#       powerWatts
#     }
#     location {
#       name
#     }
#   }
# }
#
# Query with complex nested filters
# query GetHighPowerServers {
#   highPowerAllocationsInDatacenter(datacenter: "DC-01") {
#     id
#     machine {
#       name
#       powerWatts
#     }
#     location {
#       name
#       address {
#         city
#         type
#       }
#     }
#   }
# }
#
# Using WhereInput objects (generated automatically)
# query GetAllocationsWithFilters {
#   allocationsWithFilters(
#     machineName: "Server-01"
#     machineType: "Server"
#     status: "active"
#   ) {
#     id
#     machine {
#       name
#       type
#     }
#     status
#   }
# }

if __name__ == "__main__":
    print("Nested Object Filtering Examples")
    print("================================")
    print()
    print("This example demonstrates FraiseQL v0.11.6+ nested object filtering.")
    print("Run with: fraiseql run examples.nested_filtering_example")
    print()
    print("Key features:")
    print("- Filter on nested JSONB objects: machine.name, location.address.city")
    print("- Support for all operators: eq, neq, gt, gte, lt, lte, in, notin, etc.")
    print("- Both dictionary and GraphQL WhereInput approaches")
    print("- Multiple nesting levels supported")
    print("- Type-safe filtering with generated WhereInput classes")
