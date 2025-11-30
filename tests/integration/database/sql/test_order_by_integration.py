import pytest

"""Integration test for ORDER BY functionality in GraphQL queries."""

import uuid
from datetime import datetime

import fraiseql
from fraiseql.sql import (
    BooleanFilter,
    OrderDirection,
    StringFilter,
    create_graphql_order_by_input,
    create_graphql_where_input,
)

pytestmark = [pytest.mark.integration, pytest.mark.database]


# Define test types


@pytest.mark.unit
@fraiseql.type
class Machine:
    id: uuid.UUID
    name: str
    is_current: bool = False
    last_maintenance: datetime


@fraiseql.type
class Allocation:
    id: uuid.UUID
    machine: Machine | None
    user_email: str
    status: str
    allocated_at: datetime


class TestOrderByIntegration:
    """Test ORDER BY integration with GraphQL queries."""

    def test_combined_where_and_order_by(self) -> None:
        """Test using WHERE and ORDER BY together."""
        # Create input types
        MachineWhereInput = create_graphql_where_input(Machine)
        MachineOrderByInput = create_graphql_order_by_input(Machine)
        AllocationWhereInput = create_graphql_where_input(Allocation)
        AllocationOrderByInput = create_graphql_order_by_input(Allocation)

        # Create a query with both filtering and ordering
        where_input = AllocationWhereInput(
            status=StringFilter(eq="active"),
            machine=MachineWhereInput(is_current=BooleanFilter(eq=True)),
        )

        order_by_input = AllocationOrderByInput(
            allocated_at=OrderDirection.DESC, machine=MachineOrderByInput(name=OrderDirection.ASC)
        )

        # Convert to SQL
        sql_where = where_input._to_sql_where()
        sql_order_by = order_by_input._to_sql_order_by()

        # Verify conversions
        assert sql_where is not None
        assert sql_order_by is not None

        # Check order by has correct instructions
        assert len(sql_order_by.instructions) == 2
        fields = [(i.field, i.direction) for i in sql_order_by.instructions]
        assert ("allocated_at", OrderDirection.DESC) in fields
        assert ("machine.name", OrderDirection.ASC) in fields

    def test_graphql_query_example(self) -> None:
        """Demonstrate how it would work in a GraphQL query."""
        # Create input types
        AllocationWhereInput = create_graphql_where_input(Allocation)
        AllocationOrderByInput = create_graphql_order_by_input(Allocation)
        MachineOrderByInput = create_graphql_order_by_input(Machine)

        # Simulate GraphQL resolver
        async def allocations(
            info,
            where: AllocationWhereInput | None = None,
            order_by: AllocationOrderByInput | None = None,
            limit: int = 20,
        ) -> list[Allocation]:
            # Convert inputs to SQL
            where._to_sql_where() if where else None
            sql_order_by = order_by._to_sql_order_by() if order_by else None

            # This would be passed to the database
            # return await info.context["db"].find(
            #     "allocation_view"
            #     where=sql_where
            #     order_by=sql_order_by
            #     limit=limit
            # )

            # For test, just verify the SQL generation
            if sql_order_by:
                sql_string = sql_order_by.to_sql("data").as_string(None)
                assert "ORDER BY" in sql_string

            return []  # Mock return

        # Test the resolver with order by
        AllocationOrderByInput(
            allocated_at=OrderDirection.DESC,
            user_email=OrderDirection.ASC,
            machine=MachineOrderByInput(last_maintenance=OrderDirection.DESC),
        )

        # Would be called like this in GraphQL:
        # query {
        #   allocations(
        #     orderBy: {
        #       allocatedAt: DESC
        #       userEmail: ASC
        #       machine: {
        #         lastMaintenance: DESC
        #       }
        #     }
        #   ) {
        #     id
        #     userEmail
        #   }
        # }

    def test_multiple_sort_criteria(self) -> None:
        """Test ordering by multiple fields with proper precedence."""
        MachineOrderByInput = create_graphql_order_by_input(Machine)

        # Primary sort by is_current, secondary by name, tertiary by last_maintenance
        order_by = MachineOrderByInput(
            is_current=OrderDirection.DESC,  # Active machines first
            name=OrderDirection.ASC,  # Then by name A-Z
            last_maintenance=OrderDirection.DESC,  # Most recent maintenance first
        )

        sql_order_by = order_by._to_sql_order_by()

        # Verify all three sort criteria are present
        assert len(sql_order_by.instructions) == 3

        # Generate SQL to verify format
        sql_string = sql_order_by.to_sql("data").as_string(None)
        assert "ORDER BY" in sql_string
        # Updated to use table alias for proper type handling
        assert "data -> 'is_current' DESC" in sql_string
        assert "data -> 'name' ASC" in sql_string
        assert "data -> 'last_maintenance' DESC" in sql_string

    def test_order_by_with_pagination(self) -> None:
        """Test ORDER BY with pagination patterns."""
        AllocationOrderByInput = create_graphql_order_by_input(Allocation)

        # For consistent pagination, order by a unique field last
        order_by = AllocationOrderByInput(
            allocated_at=OrderDirection.DESC,
            id=OrderDirection.ASC,  # Ensures stable sort
        )

        sql_order_by = order_by._to_sql_order_by()
        sql_string = sql_order_by.to_sql("data").as_string(None)

        # This ensures consistent pagination even if allocated_at has duplicates
        # Updated to use JSONB extraction
        assert "data -> 'allocated_at' DESC" in sql_string
        assert "data -> 'id' ASC" in sql_string

    def test_dynamic_order_by_from_user_input(self) -> None:
        """Test building order by from dynamic user input."""
        MachineOrderByInput = create_graphql_order_by_input(Machine)

        # Simulate user selecting sort options from UI
        user_sort_field = "name"
        user_sort_direction = "asc"

        # Build order by dynamically
        order_by_dict = {}
        if user_sort_field == "name":
            order_by_dict["name"] = (
                OrderDirection.ASC if user_sort_direction == "asc" else OrderDirection.DESC
            )
        elif user_sort_field == "last_maintenance":
            order_by_dict["last_maintenance"] = (
                OrderDirection.ASC if user_sort_direction == "asc" else OrderDirection.DESC
            )

        # Create order by input
        order_by = MachineOrderByInput(**order_by_dict)

        sql_order_by = order_by._to_sql_order_by()
        assert len(sql_order_by.instructions) == 1
        assert sql_order_by.instructions[0].field == "name"
        assert sql_order_by.instructions[0].direction == OrderDirection.ASC
