"""Test that the nested object tenant_id fix works correctly."""

import pytest
from uuid import UUID
from typing import Any, Optional
import fraiseql
from fraiseql import type, query
from graphql import GraphQLResolveInfo, graphql
from fraiseql import build_fraiseql_schema
from unittest.mock import AsyncMock, MagicMock


@type(sql_source="organizations")
class Organization:
    """Organization type with sql_source."""
    id: UUID
    name: str
    identifier: str
    status: str


@type(sql_source="users")
class User:
    """User type with embedded organization in JSONB data."""
    id: UUID
    first_name: str
    last_name: str
    email_address: str
    organization: Optional[Organization] = None  # This is embedded in data column


@query
async def user(info: GraphQLResolveInfo) -> Optional[User]:
    """Query to get the current user."""
    # Return a mock user with embedded organization data
    return User(
        id=UUID("75736572-0000-0000-0000-000000000000"),
        first_name="Alice",
        last_name="Cooper",
        email_address="alice@example.com",
        organization=Organization(
            id=UUID("6f726700-0000-0000-0000-000000000000"),
            name="Test Org",
            identifier="TEST-ORG",
            status="active"
        )
    )


@pytest.mark.asyncio
async def test_nested_object_with_sql_source_no_tenant_id_error():
    """Test that nested objects with sql_source don't require tenant_id when data is embedded."""

    # Create schema
    schema = build_fraiseql_schema(
        query_types=[user, User, Organization]
    )

    # GraphQL query requesting nested organization
    query_str = """
    query GetUser {
      user {
        id
        firstName
        lastName
        emailAddress
        organization {
          id
          name
          identifier
          status
        }
      }
    }
    """

    # Mock database connection
    mock_db = AsyncMock()
    mock_db.find_one = AsyncMock(return_value=None)  # Should not be called for embedded data

    context = {
        "db": mock_db,
        # Note: NOT providing tenant_id in context
    }

    # Execute query
    result = await graphql(
        schema,
        query_str,
        context_value=context
    )

    # With the fix, there should be no errors about missing tenant_id
    if result.errors:
        error_messages = [str(e) for e in result.errors]
        print(f"Errors found: {error_messages}")
        # The bug would cause "missing a required argument: 'tenant_id'" error
        assert not any("tenant_id" in msg for msg in error_messages), \
            "Unexpected tenant_id error - the bug is still present!"

    # Verify the data was returned correctly
    assert result.data is not None
    assert result.data["user"] is not None
    assert result.data["user"]["organization"] is not None
    assert result.data["user"]["organization"]["name"] == "Test Org"
    assert result.data["user"]["organization"]["identifier"] == "TEST-ORG"

    # Verify that find_one was NOT called (data was embedded, not queried)
    mock_db.find_one.assert_not_called()

    print("✓ Test passed: No tenant_id error for embedded organization with sql_source")


@pytest.mark.asyncio
async def test_smart_resolver_prefers_embedded_data():
    """Test that the smart resolver uses embedded data when available."""

    @type(sql_source="departments")
    class Department:
        id: UUID
        name: str
        code: str

    @type(sql_source="employees")
    class Employee:
        id: UUID
        name: str
        department: Optional[Department] = None

    @query
    async def employee(info: GraphQLResolveInfo) -> Optional[Employee]:
        # Return employee with embedded department
        return Employee(
            id=UUID("11111111-1111-1111-1111-111111111111"),
            name="Bob Smith",
            department=Department(
                id=UUID("22222222-2222-2222-2222-222222222222"),
                name="Engineering",
                code="ENG"
            )
        )

    schema = build_fraiseql_schema(
        query_types=[employee, Employee, Department]
    )

    query_str = """
    query GetEmployee {
      employee {
        id
        name
        department {
          id
          name
          code
        }
      }
    }
    """

    # Mock database - should not be called
    mock_db = AsyncMock()
    mock_db.find_one = AsyncMock(return_value=None)

    context = {"db": mock_db}

    result = await graphql(
        schema,
        query_str,
        context_value=context
    )

    # Should succeed without errors
    assert result.errors is None or len(result.errors) == 0
    assert result.data["employee"]["department"]["name"] == "Engineering"

    # Database should not be queried for embedded data
    mock_db.find_one.assert_not_called()

    print("✓ Smart resolver correctly uses embedded data without querying sql_source")


@pytest.mark.asyncio
async def test_smart_resolver_queries_when_no_embedded_data():
    """Test that the smart resolver queries sql_source when data is not embedded."""

    @type(sql_source="departments")
    class Department:
        id: UUID
        name: str
        code: str

    @type(sql_source="employees")
    class Employee:
        id: UUID
        name: str
        department: Optional[Department] = None
        department_id: Optional[UUID] = None  # Foreign key field

    @query
    async def employee(info: GraphQLResolveInfo) -> Optional[Employee]:
        # Return employee WITHOUT embedded department, but with FK
        return Employee(
            id=UUID("11111111-1111-1111-1111-111111111111"),
            name="Bob Smith",
            department=None,  # No embedded data
            department_id=UUID("22222222-2222-2222-2222-222222222222")  # But has FK
        )

    schema = build_fraiseql_schema(
        query_types=[employee, Employee, Department]
    )

    query_str = """
    query GetEmployee {
      employee {
        id
        name
        department {
          id
          name
          code
        }
      }
    }
    """

    # Mock database - should be called this time
    mock_db = AsyncMock()
    mock_db.find_one = AsyncMock(return_value={
        "id": UUID("22222222-2222-2222-2222-222222222222"),
        "name": "Engineering",
        "code": "ENG"
    })

    context = {
        "db": mock_db,
        "tenant_id": UUID("33333333-3333-3333-3333-333333333333")  # Provide tenant_id
    }

    result = await graphql(
        schema,
        query_str,
        context_value=context
    )

    # Should succeed - smart resolver queries when no embedded data
    if result.errors:
        print(f"Errors: {result.errors}")

    # Data should be fetched from database
    assert result.data is not None
    assert result.data["employee"]["department"] is not None
    assert result.data["employee"]["department"]["name"] == "Engineering"

    # Verify database was queried
    mock_db.find_one.assert_called_once()
    call_args = mock_db.find_one.call_args
    assert call_args[0][0] == "departments"  # Table name
    assert "id" in call_args[1]  # Query parameters

    print("✓ Smart resolver correctly queries sql_source when no embedded data")


if __name__ == "__main__":
    import asyncio
    asyncio.run(test_nested_object_with_sql_source_no_tenant_id_error())
    asyncio.run(test_smart_resolver_prefers_embedded_data())
    asyncio.run(test_smart_resolver_queries_when_no_embedded_data())
