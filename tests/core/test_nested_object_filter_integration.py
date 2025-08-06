"""Integration test for nested object filtering in GraphQL where inputs."""

import uuid
from datetime import datetime

import fraiseql
from fraiseql.sql import (
    BooleanFilter,
    StringFilter,
    UUIDFilter,
    create_graphql_where_input,
)


# Define test types
@fraiseql.type
class Machine:
    id: uuid.UUID
    name: str
    is_current: bool = False


@fraiseql.type
class Allocation:
    id: uuid.UUID
    machine: Machine | None
    status: str
    created_at: datetime


class TestNestedObjectFilterIntegration:
    """Test nested object filtering works end-to-end."""

    def test_nested_filter_conversion_to_sql(self):
        """Test that nested filters are properly converted to SQL where conditions."""
        # Create where input types
        MachineWhereInput = create_graphql_where_input(Machine)
        AllocationWhereInput = create_graphql_where_input(Allocation)

        # Create a nested filter
        where_input = AllocationWhereInput(
            machine=MachineWhereInput(
                is_current=BooleanFilter(eq=True), name=StringFilter(contains="Server")
            ),
            status=StringFilter(eq="active"),
        )

        # Convert to SQL where type
        sql_where = where_input._to_sql_where()

        # Verify the conversion worked
        assert hasattr(sql_where, "machine")
        assert hasattr(sql_where, "status")

        # The machine field should contain the nested where conditions
        assert sql_where.machine is not None
        assert sql_where.status == {"eq": "active"}

        # Generate SQL to ensure it doesn't error
        sql = sql_where.to_sql()
        assert sql is not None

    def test_nested_filter_with_none_values(self):
        """Test that None values in nested filters are handled correctly."""
        MachineWhereInput = create_graphql_where_input(Machine)
        AllocationWhereInput = create_graphql_where_input(Allocation)

        # Test with None machine filter
        where_input = AllocationWhereInput(
            id=UUIDFilter(eq=uuid.uuid4()), machine=None, status=StringFilter(eq="pending")
        )

        sql_where = where_input._to_sql_where()
        assert sql_where.machine == {}  # No filter on machine
        assert sql_where.status == {"eq": "pending"}

    def test_deeply_nested_filtering(self):
        """Test multiple levels of nested filtering."""

        @fraiseql.type
        class Location:
            id: uuid.UUID
            city: str
            country: str

        @fraiseql.type
        class MachineWithLocation:
            id: uuid.UUID
            name: str
            location: Location | None

        @fraiseql.type
        class AllocationDeep:
            id: uuid.UUID
            machine: MachineWithLocation | None

        # Create where inputs
        LocationWhereInput = create_graphql_where_input(Location)
        MachineWithLocationWhereInput = create_graphql_where_input(MachineWithLocation)
        AllocationDeepWhereInput = create_graphql_where_input(AllocationDeep)

        # Create deeply nested filter
        where_input = AllocationDeepWhereInput(
            machine=MachineWithLocationWhereInput(
                name=StringFilter(startswith="VM"),
                location=LocationWhereInput(
                    city=StringFilter(eq="Seattle"), country=StringFilter(eq="USA")
                ),
            )
        )

        # Convert and verify
        sql_where = where_input._to_sql_where()
        assert hasattr(sql_where, "machine")
        assert sql_where.machine is not None

    def test_mixed_scalar_and_nested_filters(self):
        """Test mixing scalar and nested object filters."""
        MachineWhereInput = create_graphql_where_input(Machine)
        AllocationWhereInput = create_graphql_where_input(Allocation)

        # Mix scalar and nested filters
        test_id = uuid.uuid4()
        where_input = AllocationWhereInput(
            id=UUIDFilter(eq=test_id),
            status=StringFilter(in_=["active", "pending"]),
            machine=MachineWhereInput(
                is_current=BooleanFilter(eq=True), name=StringFilter(neq="deprecated")
            ),
        )

        sql_where = where_input._to_sql_where()

        # Verify all filters are present
        assert sql_where.id == {"eq": test_id}
        assert sql_where.status == {"in": ["active", "pending"]}
        assert sql_where.machine is not None

    def test_empty_nested_filter(self):
        """Test that empty nested filters are handled correctly."""
        MachineWhereInput = create_graphql_where_input(Machine)
        AllocationWhereInput = create_graphql_where_input(Allocation)

        # Create filter with empty nested filter
        where_input = AllocationWhereInput(
            status=StringFilter(eq="active"),
            machine=MachineWhereInput(),  # Empty filter
        )

        sql_where = where_input._to_sql_where()

        # Empty nested filter should create a nested where object with empty fields
        assert sql_where.status == {"eq": "active"}
        assert sql_where.machine is not None
        # The nested where object should have empty operator dicts
        assert hasattr(sql_where.machine, "id")
        assert sql_where.machine.id == {}
        assert sql_where.machine.name == {}
        assert sql_where.machine.is_current == {}
