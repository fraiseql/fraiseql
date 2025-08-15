"""Test to reproduce the first query returning null issue."""

import uuid
from typing import Optional
import pytest
from httpx import ASGITransport, AsyncClient
from fastapi.testclient import TestClient

import fraiseql
from fraiseql import query
from fraiseql.types import fraise_type
from fraiseql.fastapi import create_fraiseql_app
from graphql import GraphQLResolveInfo


# Define the User type
@fraise_type
class User:
    id: uuid.UUID
    name: str
    email: str


# Mock users data
MOCK_USERS = {
    uuid.UUID("11111111-1111-1111-1111-111111111111"): {
        "id": uuid.UUID("11111111-1111-1111-1111-111111111111"),
        "name": "Alice",
        "email": "alice@example.com",
        "tenant_id": uuid.UUID("22222222-2222-2222-2222-222222222222"),
    },
    uuid.UUID("33333333-3333-3333-3333-333333333333"): {
        "id": uuid.UUID("33333333-3333-3333-3333-333333333333"),
        "name": "Bob",
        "email": "bob@example.com",
        "tenant_id": uuid.UUID("22222222-2222-2222-2222-222222222222"),
    },
}


# Define query resolver
@query
async def user(
    info: GraphQLResolveInfo,
    id: uuid.UUID
) -> Optional[User]:
    """Get a user by ID."""
    print(f"DEBUG: user resolver called with id={id} (type: {type(id)})")

    # Convert to UUID if string
    if isinstance(id, str):
        id = uuid.UUID(id)

    # Mock database lookup
    user_data = MOCK_USERS.get(id)
    print(f"DEBUG: Mock database result: {user_data}")

    if user_data:
        return User(id=user_data["id"], name=user_data["name"], email=user_data["email"])
    return None


@query
async def users(
    info: GraphQLResolveInfo,
    limit: int = 10
) -> list[User]:
    """Get list of users."""
    print(f"DEBUG: users resolver called with limit={limit}")

    # Return mock users
    result = []
    for user_data in list(MOCK_USERS.values())[:limit]:
        result.append(User(id=user_data["id"], name=user_data["name"], email=user_data["email"]))

    print(f"DEBUG: Returning {len(result)} users")
    return result


def test_first_query_returns_null_simple():
    """Test that demonstrates the first query returning null issue using simple mocked queries."""

    # Create the app without database
    app = create_fraiseql_app(
        database_url="postgresql://test/test",  # Dummy URL since we're mocking
        types=[User],
        production=False
    )

    # Use TestClient for simpler testing
    with TestClient(app) as client:
        # Query 1: Get user by ID (FIRST QUERY - might return null)
        query1 = {
            "query": """
                query GetUser {
                    user(id: "11111111-1111-1111-1111-111111111111") {
                        id
                        name
                        email
                    }
                }
            """
        }

        print("\n" + "=" * 60)
        print("FIRST QUERY EXECUTION (After App Startup)")
        print("=" * 60)

        response1 = client.post("/graphql", json=query1)
        result1 = response1.json()
        print(f"First query result: {result1}")

        # Query 2: Same query again (should work)
        print("\n" + "=" * 60)
        print("SECOND QUERY EXECUTION (Same Query)")
        print("=" * 60)

        response2 = client.post("/graphql", json=query1)
        result2 = response2.json()
        print(f"Second query result: {result2}")

        # Query 3: List query
        query3 = {
            "query": """
                query GetUsers {
                    users(limit: 5) {
                        id
                        name
                        email
                    }
                }
            """
        }

        print("\n" + "=" * 60)
        print("THIRD QUERY EXECUTION (Different Query)")
        print("=" * 60)

        response3 = client.post("/graphql", json=query3)
        result3 = response3.json()
        print(f"Third query result: {result3}")

        # Assertions
        first_query_user = result1.get('data', {}).get('user')
        second_query_user = result2.get('data', {}).get('user')
        users_list = result3.get('data', {}).get('users', [])

        print("\n" + "=" * 60)
        print("ANALYSIS")
        print("=" * 60)
        print(f"First query user: {first_query_user}")
        print(f"Second query user: {second_query_user}")
        print(f"Users list count: {len(users_list)}")

        # The bug: first query returns null, second query works
        if first_query_user is None and second_query_user is not None:
            print("\n❌ BUG CONFIRMED: First query returned null, second query worked!")
            print("This confirms the FraiseQL first query initialization issue.")
            # Make test fail to highlight the issue
            pytest.fail("First query returned null while second query worked - initialization bug confirmed")
        elif first_query_user is not None:
            print("\n✅ Bug not reproduced - first query returned data correctly")
            # This would mean the bug is fixed or not present
            pass
        else:
            print("\n⚠️ Both queries returned null - different issue")
            pytest.fail("Both queries returned null - this is a different issue")
