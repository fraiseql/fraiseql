"""Tests for nested object filtering in GraphQL where inputs."""

from __future__ import annotations

import uuid

import pytest

import fraiseql
from fraiseql.sql import (
    BooleanFilter,
    IntFilter,
    StringFilter,
    UUIDFilter,
    create_graphql_where_input,
)

pytestmark = [pytest.mark.integration, pytest.mark.database]


# Define test types at module level to avoid forward reference issues


@pytest.mark.unit
@fraiseql.type
class Machine:
    id: uuid.UUID
    is_current: bool = False
    name: str


@fraiseql.type
class Allocation:
    id: uuid.UUID
    machine: Machine | None
    status: str


@fraiseql.type
class Location:
    id: uuid.UUID
    name: str
    city: str


@fraiseql.type
class MachineWithLocation:
    id: uuid.UUID
    name: str
    location: Location | None


@fraiseql.type
class AllocationDeep:
    id: uuid.UUID
    machine: MachineWithLocation | None


# Circular reference types will be tested separately
# @fraiseql.type
# class Department:
#     id: uuid.UUID
#     name: str
#     manager: Optional["Employee"] = None

# @fraiseql.type
# class Employee:
#     id: uuid.UUID
#     name: str
#     department: Optional[Department] = None


@fraiseql.type
class Tag:
    id: uuid.UUID
    name: str


@fraiseql.type
class Product:
    id: uuid.UUID
    name: str
    tags: list[Tag]


@fraiseql.type
class Address:
    street: str
    city: str
    zip_code: str


@fraiseql.type
class User:
    id: uuid.UUID
    name: str
    email: str
    age: int
    address: Address | None
    is_active: bool


class TestNestedObjectFiltering:
    """Test nested object filtering in GraphQL where inputs."""

    def test_simple_nested_object_filter(self) -> None:
        """Test that nested object fields get proper filter types instead of StringFilter."""
        # Clear the cache first to ensure clean state
        from fraiseql.sql.graphql_where_generator import _generation_stack, _where_input_cache

        _where_input_cache.clear()
        _generation_stack.clear()

        # Create where inputs
        MachineWhereInput = create_graphql_where_input(Machine)
        AllocationWhereInput = create_graphql_where_input(Allocation)

        # Check that machine field is MachineWhereInput, not StringFilter
        allocation_fields = AllocationWhereInput.__dataclass_fields__
        assert "machine" in allocation_fields

        # The type should be Optional[MachineWhereInput], not Optional[StringFilter]
        field_type = allocation_fields["machine"].type
        # Extract the actual type from Optional
        import typing

        if hasattr(typing, "get_args"):
            args = typing.get_args(field_type)
            if args:
                actual_type = args[0]
                assert actual_type == MachineWhereInput
                assert actual_type != StringFilter

    def test_nested_object_filter_usage(self) -> None:
        """Test using nested filters in practice."""
        MachineWhereInput = create_graphql_where_input(Machine)
        AllocationWhereInput = create_graphql_where_input(Allocation)

        # Create a nested filter query
        where_input = AllocationWhereInput(
            machine=MachineWhereInput(
                is_current=BooleanFilter(eq=True), name=StringFilter(contains="Server")
            ),
            status=StringFilter(eq="active"),
        )

        # Verify the structure
        assert where_input.machine is not None
        assert where_input.machine.is_current.eq is True
        assert where_input.machine.name.contains == "Server"
        assert where_input.status.eq == "active"

    def test_deeply_nested_objects(self) -> None:
        """Test multiple levels of nesting."""
        # Create where inputs
        LocationWhereInput = create_graphql_where_input(Location)
        MachineWithLocationWhereInput = create_graphql_where_input(MachineWithLocation)
        AllocationDeepWhereInput = create_graphql_where_input(AllocationDeep)

        # Create deeply nested filter
        where_input = AllocationDeepWhereInput(
            machine=MachineWithLocationWhereInput(
                location=LocationWhereInput(city=StringFilter(eq="Seattle"))
            )
        )

        assert where_input.machine.location.city.eq == "Seattle"

    # def test_circular_reference_handling(self) -> None:
    #     """Test that circular references don't cause infinite recursion."""
    #     # This should not cause infinite recursion
    #     DepartmentWhereInput = create_graphql_where_input(Department)
    #     EmployeeWhereInput = create_graphql_where_input(Employee)

    #     # Both should be created successfully
    #     assert DepartmentWhereInput is not None
    #     assert EmployeeWhereInput is not None

    #     # Check field types
    #     assert "manager" in DepartmentWhereInput.__dataclass_fields__
    #     assert "department" in EmployeeWhereInput.__dataclass_fields__

    def test_list_of_nested_objects(self) -> None:
        """Test filtering on lists of nested objects."""
        create_graphql_where_input(Tag)
        ProductWhereInput = create_graphql_where_input(Product)

        # For lists, we might want to support 'some' or 'every' operators
        # This test documents expected behavior
        assert "tags" in ProductWhereInput.__dataclass_fields__

    def test_nested_filter_sql_conversion(self) -> None:
        """Test that nested filters convert properly to SQL where clauses."""
        MachineWhereInput = create_graphql_where_input(Machine)
        AllocationWhereInput = create_graphql_where_input(Allocation)

        where_input = AllocationWhereInput(
            machine=MachineWhereInput(is_current=BooleanFilter(eq=True))
        )

        # Convert to SQL where type
        sql_where = where_input._to_sql_where()

        # The SQL where should properly handle the nested structure
        assert hasattr(sql_where, "machine")
        # The nested filter should be converted to proper SQL conditions
        # Note: The exact SQL generation might need to handle JSON path expressions

    def test_mixed_scalar_and_object_fields(self) -> None:
        """Test types with both scalar and object fields."""
        AddressWhereInput = create_graphql_where_input(Address)
        UserWhereInput = create_graphql_where_input(User)

        # Check all fields have correct filter types

        # Scalar fields should have their respective filter types
        where = UserWhereInput()
        where.name = StringFilter(contains="John")
        where.age = IntFilter(gte=18)
        where.is_active = BooleanFilter(eq=True)

        # Nested object should have its where input type
        where.address = AddressWhereInput(city=StringFilter(eq="New York"))

        assert where.name.contains == "John"
        assert where.age.gte == 18
        assert where.is_active.eq is True
        assert where.address.city.eq == "New York"

    def test_none_handling_in_nested_filters(self) -> None:
        """Test that None values are handled correctly in nested structures."""
        create_graphql_where_input(Machine)
        AllocationWhereInput = create_graphql_where_input(Allocation)

        # Test with None machine filter
        where_input = AllocationWhereInput(id=UUIDFilter(eq=uuid.uuid4()), machine=None)

        sql_where = where_input._to_sql_where()
        assert sql_where.machine == {}  # No filter on machine

    def test_error_message_improvement(self) -> None:
        """Test that error messages are clear when using incorrect filter types."""
        AllocationWhereInput = create_graphql_where_input(Allocation)

        # This should provide a clear error or type hint
        # Currently it would accept StringFilter which is wrong
        # After fix, it should only accept MachineWhereInput or None

        # Document the expected behavior for type checking
        try:
            # This should fail or at least be caught by type checkers
            AllocationWhereInput(machine=StringFilter(eq="wrong"))  # This is incorrect
            # If this doesn't fail at runtime, type checkers should catch it
        except (TypeError, ValueError) as e:
            # Expected to fail with clear error about wrong filter type
            assert "MachineWhereInput" in str(e) or "filter type" in str(e)
