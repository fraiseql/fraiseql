"""Tests for GraphQL ORDER BY input type generator."""

from __future__ import annotations

import uuid
from datetime import datetime
from typing import Optional

import pytest

import fraiseql
from fraiseql.sql import OrderByItem, OrderDirection, create_graphql_order_by_input

pytestmark = [pytest.mark.integration, pytest.mark.database]


# Define test types at module level


@pytest.mark.unit
@fraiseql.type
class Department:
    id: uuid.UUID
    name: str
    created_at: datetime


@fraiseql.type
class Employee:
    id: uuid.UUID
    name: str
    age: int
    department: Department | None
    hire_date: datetime


@fraiseql.type
class Document:
    id: uuid.UUID
    title: str
    embedding: list[float]  # Vector field
    created_at: datetime


@fraiseql.type
class Company:
    id: uuid.UUID
    name: str
    ceo: Employee | None
    founded_at: datetime


# Note: Circular reference types commented out due to Python type annotation limitations
# They are tested separately in test_circular_reference_handling


class TestGraphQLOrderByGenerator:
    """Test GraphQL ORDER BY input type generation."""

    def test_simple_order_by_input_generation(self) -> None:
        """Test generating order by input for simple types."""
        # Clear cache
        from fraiseql.sql.graphql_order_by_generator import _generation_stack, _order_by_input_cache

        _order_by_input_cache.clear()
        _generation_stack.clear()

        # Create order by input
        DepartmentOrderByInput = create_graphql_order_by_input(Department)

        # Check fields
        fields = DepartmentOrderByInput.__dataclass_fields__
        assert "id" in fields
        assert "name" in fields
        assert "created_at" in fields

        # Check field types - should be Optional[OrderDirection]
        import typing

        if hasattr(typing, "get_args"):
            for field in fields.values():
                args = typing.get_args(field.type)
                if args:
                    assert args[0] == OrderDirection

    def test_order_by_input_usage(self) -> None:
        """Test using order by input in practice."""
        DepartmentOrderByInput = create_graphql_order_by_input(Department)

        # Create order by query
        order_by = DepartmentOrderByInput(name=OrderDirection.ASC, created_at=OrderDirection.DESC)

        assert order_by.name == OrderDirection.ASC
        assert order_by.created_at == OrderDirection.DESC
        assert order_by.id is None  # Not set

    def test_nested_object_order_by(self) -> None:
        """Test order by with nested objects."""
        DepartmentOrderByInput = create_graphql_order_by_input(Department)
        EmployeeOrderByInput = create_graphql_order_by_input(Employee)

        # Check that department field is DepartmentOrderByInput
        fields = EmployeeOrderByInput.__dataclass_fields__
        assert "department" in fields

        # The type should be Optional[DepartmentOrderByInput]
        import typing

        if hasattr(typing, "get_args"):
            field_type = fields["department"].type
            args = typing.get_args(field_type)
            if args:
                assert args[0] == DepartmentOrderByInput

    def test_nested_order_by_usage(self) -> None:
        """Test using nested order by in practice."""
        DepartmentOrderByInput = create_graphql_order_by_input(Department)
        EmployeeOrderByInput = create_graphql_order_by_input(Employee)

        # Create nested order by
        order_by = EmployeeOrderByInput(
            name=OrderDirection.ASC, department=DepartmentOrderByInput(name=OrderDirection.DESC)
        )

        assert order_by.name == OrderDirection.ASC
        assert order_by.department.name == OrderDirection.DESC

    def test_sql_conversion_simple(self) -> None:
        """Test converting order by input to SQL."""
        DepartmentOrderByInput = create_graphql_order_by_input(Department)

        order_by = DepartmentOrderByInput(name=OrderDirection.ASC, created_at=OrderDirection.DESC)

        # Convert to SQL
        sql_order_by = order_by._to_sql_order_by()

        assert sql_order_by is not None
        assert len(sql_order_by.instructions) == 2
        assert sql_order_by.instructions[0].field == "name"
        assert sql_order_by.instructions[0].direction == OrderDirection.ASC
        assert sql_order_by.instructions[1].field == "created_at"
        assert sql_order_by.instructions[1].direction == OrderDirection.DESC

    def test_sql_conversion_nested(self) -> None:
        """Test converting nested order by to SQL."""
        DepartmentOrderByInput = create_graphql_order_by_input(Department)
        EmployeeOrderByInput = create_graphql_order_by_input(Employee)

        order_by = EmployeeOrderByInput(
            name=OrderDirection.ASC,
            department=DepartmentOrderByInput(
                name=OrderDirection.DESC, created_at=OrderDirection.ASC
            ),
        )

        # Convert to SQL
        sql_order_by = order_by._to_sql_order_by()

        assert sql_order_by is not None
        assert len(sql_order_by.instructions) == 3

        # Check the generated field paths
        fields_and_directions = [
            (instr.field, instr.direction) for instr in sql_order_by.instructions
        ]
        assert ("name", OrderDirection.ASC) in fields_and_directions
        assert ("department.name", OrderDirection.DESC) in fields_and_directions
        assert ("department.created_at", OrderDirection.ASC) in fields_and_directions

    def test_circular_reference_handling(self) -> None:
        """Test that circular references don't cause infinite recursion."""
        # Clear cache
        from fraiseql.sql.graphql_order_by_generator import _generation_stack, _order_by_input_cache

        _order_by_input_cache.clear()
        _generation_stack.clear()

        # Define circular reference types locally to avoid import issues
        @fraiseql.type
        class LocalPost:
            id: uuid.UUID
            title: str
            created_at: datetime

        @fraiseql.type
        class LocalAuthor:
            id: uuid.UUID
            name: str

        # Manually add circular references after definition
        LocalPost.__annotations__["author"] = Optional[LocalAuthor]
        LocalAuthor.__annotations__["latest_post"] = Optional[LocalPost]

        # This should not cause infinite recursion
        PostOrderByInput = create_graphql_order_by_input(LocalPost)
        AuthorOrderByInput = create_graphql_order_by_input(LocalAuthor)

        # Both should be created successfully
        assert PostOrderByInput is not None
        assert AuthorOrderByInput is not None

        # Check field types
        assert "author" in PostOrderByInput.__dataclass_fields__
        assert "latest_post" in AuthorOrderByInput.__dataclass_fields__

    def test_deep_nesting(self) -> None:
        """Test multiple levels of nesting."""
        CompanyOrderByInput = create_graphql_order_by_input(Company)
        EmployeeOrderByInput = create_graphql_order_by_input(Employee)
        DepartmentOrderByInput = create_graphql_order_by_input(Department)

        # Create deeply nested order by
        order_by = CompanyOrderByInput(
            name=OrderDirection.ASC,
            ceo=EmployeeOrderByInput(
                name=OrderDirection.DESC, department=DepartmentOrderByInput(name=OrderDirection.ASC)
            ),
        )

        # Convert to SQL
        sql_order_by = order_by._to_sql_order_by()

        assert sql_order_by is not None
        # Should have fields: name, ceo.name, ceo.department.name
        assert len(sql_order_by.instructions) == 3

    def test_list_based_order_by(self) -> None:
        """Test list-based order by approach."""
        # Create list of order by items
        order_by_list = [
            OrderByItem(field="name", direction=OrderDirection.ASC),
            OrderByItem(field="age", direction=OrderDirection.DESC),
            OrderByItem(field="department.name", direction=OrderDirection.ASC),
        ]

        # Convert to SQL using the converter
        from fraiseql.sql.graphql_order_by_generator import _convert_order_by_input_to_sql

        sql_order_by = _convert_order_by_input_to_sql(order_by_list)

        assert sql_order_by is not None
        assert len(sql_order_by.instructions) == 3
        assert sql_order_by.instructions[0].field == "name"
        assert sql_order_by.instructions[0].direction == OrderDirection.ASC
        assert sql_order_by.instructions[1].field == "age"
        assert sql_order_by.instructions[1].direction == OrderDirection.DESC
        assert sql_order_by.instructions[2].field == "department.name"
        assert sql_order_by.instructions[2].direction == OrderDirection.ASC

    def test_vector_order_by_input_generation(self) -> None:
        """Test generating order by input for types with vector fields."""
        # Clear cache
        from fraiseql.sql.graphql_order_by_generator import (
            VectorOrderBy,
            _generation_stack,
            _order_by_input_cache,
        )

        _order_by_input_cache.clear()
        _generation_stack.clear()

        # Create order by input for Document (which has embedding field)
        DocumentOrderByInput = create_graphql_order_by_input(Document)

        # Check fields
        fields = DocumentOrderByInput.__dataclass_fields__
        assert "id" in fields
        assert "title" in fields
        assert "embedding" in fields
        assert "created_at" in fields

        # Check that embedding field has VectorOrderBy type
        embedding_field = fields["embedding"]
        import typing

        if hasattr(typing, "get_args"):
            args = typing.get_args(embedding_field.type)
            if args:
                # Should be Optional[VectorOrderBy]
                assert args[0] == VectorOrderBy

    def test_vector_order_by_usage(self) -> None:
        """Test using vector order by input."""
        from fraiseql.sql.graphql_order_by_generator import VectorOrderBy

        DocumentOrderByInput = create_graphql_order_by_input(Document)

        # Create order by with vector distance
        vector_order_by = VectorOrderBy(cosine_distance=[0.1, 0.2, 0.3])
        order_by = DocumentOrderByInput(title=OrderDirection.ASC, embedding=vector_order_by)

        # Convert to SQL
        from fraiseql.sql.graphql_order_by_generator import _convert_order_by_input_to_sql

        sql_order_by = _convert_order_by_input_to_sql(order_by)

        assert sql_order_by is not None
        assert len(sql_order_by.instructions) == 2

        # Check title ordering
        title_instruction = next(i for i in sql_order_by.instructions if i.field == "title")
        assert title_instruction.direction == OrderDirection.ASC

        # Check embedding ordering
        embedding_instruction = next(i for i in sql_order_by.instructions if "embedding" in i.field)
        assert embedding_instruction.field == "embedding.cosine_distance"
        assert embedding_instruction.value == [0.1, 0.2, 0.3]
        assert embedding_instruction.direction == OrderDirection.ASC

    def test_empty_order_by(self) -> None:
        """Test handling empty order by input."""
        DepartmentOrderByInput = create_graphql_order_by_input(Department)

        # Create empty order by
        order_by = DepartmentOrderByInput()

        # Convert to SQL
        sql_order_by = order_by._to_sql_order_by()

        # Should return None or empty OrderBySet
        assert sql_order_by is None or len(sql_order_by.instructions) == 0

    def test_custom_input_name(self) -> None:
        """Test creating order by input with custom name."""
        CustomOrderBy = create_graphql_order_by_input(Department, name="CustomDeptOrder")

        assert CustomOrderBy.__name__ == "CustomDeptOrder"
        assert "name" in CustomOrderBy.__dataclass_fields__

    def test_sql_query_generation(self) -> None:
        """Test that generated SQL is valid."""
        DepartmentOrderByInput = create_graphql_order_by_input(Department)

        order_by = DepartmentOrderByInput(name=OrderDirection.ASC, created_at=OrderDirection.DESC)

        sql_order_by = order_by._to_sql_order_by()
        sql_string = sql_order_by.to_sql("data").as_string(None)

        # Should generate valid ORDER BY clause with JSONB extraction
        assert "ORDER BY" in sql_string
        assert "data -> 'name' ASC" in sql_string
        assert "data -> 'created_at' DESC" in sql_string

    def test_integration_with_repository(self) -> None:
        """Test how order by would integrate with repository pattern."""
        EmployeeOrderByInput = create_graphql_order_by_input(Employee)

        # Simulate GraphQL resolver
        async def get_employees(info, order_by: EmployeeOrderByInput | None = None) -> None:
            if order_by:
                order_by._to_sql_order_by()
                # This would be passed to repository
                # return await db.find("employee_view", order_by=sql_order_by)

            # For test, just verify conversion works
            return []

        # Test various order by scenarios
        order_by = EmployeeOrderByInput(
            hire_date=OrderDirection.DESC,
            department=create_graphql_order_by_input(Department)(name=OrderDirection.ASC),
        )

        sql_order_by = order_by._to_sql_order_by()
        assert sql_order_by is not None
        assert len(sql_order_by.instructions) == 2
