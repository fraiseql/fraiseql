import pytest

"""Integration test demonstrating the ORDER BY bug fix."""

import uuid

import fraiseql
from fraiseql.sql import OrderDirection, create_graphql_order_by_input

pytestmark = [pytest.mark.integration, pytest.mark.database]


# Define test types


@pytest.mark.unit
@fraiseql.type
class Department:
    id: uuid.UUID
    name: str
    location: str


@fraiseql.type
class Employee:
    id: uuid.UUID
    name: str
    email: str
    department: Department | None
    hire_date: str


# Create ORDER BY inputs
DepartmentOrderBy = create_graphql_order_by_input(Department)
EmployeeOrderBy = create_graphql_order_by_input(Employee)


def test_real_world_graphql_order_by_usage() -> None:
    """Test the real-world usage pattern from the bug report."""
    # Simulate what GraphQL frameworks pass to the resolver
    # This is typically a dict, not an instance of our OrderByInputClass
    order_by_from_graphql = {"name": "DESC", "department": {"name": "ASC"}}

    # Simulate a resolver function
    async def employees(info, order_by=None) -> None:
        # This is what happens in a real resolver
        db = info.context["db"]

        # The fix ensures db.find can handle the dict directly
        return await db.find(
            """employee_view""",
            order_by=order_by,  # This now works with dict input!
        )

    # Test the conversion directly
    from fraiseql.sql.graphql_order_by_generator import _convert_order_by_input_to_sql

    sql_order_by = _convert_order_by_input_to_sql(order_by_from_graphql)
    assert sql_order_by is not None

    # Verify SQL generation with JSONB extraction
    sql_string = sql_order_by.to_sql("data").as_string(None)
    assert "ORDER BY" in sql_string
    assert "data -> 'name' DESC" in sql_string
    assert "data -> 'department' -> 'name' ASC" in sql_string


def test_simple_order_by_dict() -> None:
    """Test simple ORDER BY as reported in the bug."""
    # The exact case from the bug report
    order_by = {"name": "DESC"}

    from fraiseql.sql.graphql_order_by_generator import _convert_order_by_input_to_sql

    sql_order_by = _convert_order_by_input_to_sql(order_by)
    assert sql_order_by is not None
    assert len(sql_order_by.instructions) == 1
    assert sql_order_by.instructions[0].field == "name"
    assert sql_order_by.instructions[0].direction == OrderDirection.DESC


def test_enum_values_in_dict() -> None:
    """Test that OrderDirection enum values work in dicts."""
    # Some GraphQL libraries might pass the enum directly
    order_by = {"name": OrderDirection.DESC, "email": OrderDirection.ASC}

    from fraiseql.sql.graphql_order_by_generator import _convert_order_by_input_to_sql

    sql_order_by = _convert_order_by_input_to_sql(order_by)
    assert sql_order_by is not None
    assert len(sql_order_by.instructions) == 2
