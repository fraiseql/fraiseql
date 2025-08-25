import pytest

"""Tests for field-level authorization in FraiseQL - Fixed version."""

from graphql import graphql_sync

import fraiseql
from fraiseql import query
from fraiseql.decorators import field
from fraiseql.gql.schema_builder import build_fraiseql_schema
from fraiseql.security.field_auth import authorize_field


@pytest.mark.security
def test_field_authorization_basic():
    """Test basic field authorization with GraphQL execution."""

    @fraiseql.type
    class User:
        name: str
        email_value: str = "default@example.com"

        @authorize_field(lambda info: info.context.get("is_admin", False))
        @field
        def email(self) -> str:
            return self.email_value

    @query
    def get_user(info) -> User:
        return User(name="John Doe", email_value="john@example.com")

    schema = build_fraiseql_schema(query_types=[get_user])

    # Test with admin access
    query_str = """
    query {
        getUser {
            name
            email
        }
    }
    """
    result = graphql_sync(schema, query_str, context_value={"is_admin": True})
    assert result.errors is None
    assert result.data == {"getUser": {"name": "John Doe", "email": "john@example.com"}}

    # Test without admin access
    result = graphql_sync(schema, query_str, context_value={"is_admin": False})
    assert result.errors is not None
    assert len(result.errors) == 1
    assert "Not authorized to access field" in str(result.errors[0])
    assert result.data == {"getUser": {"name": "John Doe", "email": None}}


def test_field_authorization_with_custom_message():
    """Test field authorization with custom error message."""

    @fraiseql.type
    class User:
        name: str
        phone_value: str = ""

        @authorize_field(
            lambda info: info.context.get("is_admin", False),
            error_message="Admin access required to view phone number",
        )
        @field
        def phone(self) -> str:
            return self.phone_value

    @query
    def get_user(info) -> User:
        return User(name="Jane Doe", phone_value="+1234567890")

    schema = build_fraiseql_schema(query_types=[get_user])

    query_str = """
    query {
        getUser {
            name
            phone
        }
    }
    """

    # Test without admin access - should see custom error message
    result = graphql_sync(schema, query_str, context_value={"is_admin": False})
    assert result.errors is not None
    assert "Admin access required to view phone number" in str(result.errors[0])


def test_field_authorization_multiple_fields():
    """Test authorization on multiple fields."""

    @fraiseql.type
    class User:
        name: str
        email_value: str = ""
        phone_value: str = ""
        ssn_value: str = ""

        @authorize_field(lambda info: info.context.get("authenticated", False))
        @field
        def email(self) -> str:
            return self.email_value

        @authorize_field(lambda info: info.context.get("is_admin", False))
        @field
        def phone(self) -> str:
            return self.phone_value

        @authorize_field(lambda info: info.context.get("is_superadmin", False))
        @field
        def ssn(self) -> str:
            return self.ssn_value

    @query
    def get_user(info) -> User:
        return User(
            name="Bob Smith",
            email_value="bob@example.com",
            phone_value="+9876543210",
            ssn_value="123-45-6789",
        )

    schema = build_fraiseql_schema(query_types=[get_user])

    query_str = """
    query {
        getUser {
            name
            email
            phone
            ssn
        }
    }
    """

    # Test with different permission levels
    # 1. Unauthenticated - can only see name
    result = graphql_sync(schema, query_str, context_value={})
    assert result.data == {
        "getUser": {"name": "Bob Smith", "email": None, "phone": None, "ssn": None}
    }
    assert len(result.errors) == 3

    # 2. Authenticated - can see email
    result = graphql_sync(schema, query_str, context_value={"authenticated": True})
    assert result.data == {
        "getUser": {"name": "Bob Smith", "email": "bob@example.com", "phone": None, "ssn": None}
    }
    assert len(result.errors) == 2

    # 3. Admin - can see email and phone
    result = graphql_sync(
        schema, query_str, context_value={"authenticated": True, "is_admin": True}
    )
    assert result.data == {
        "getUser": {
            "name": "Bob Smith",
            "email": "bob@example.com",
            "phone": "+9876543210",
            "ssn": None,
        }
    }
    assert len(result.errors) == 1

    # 4. Superadmin - can see everything
    result = graphql_sync(
        schema,
        query_str,
        context_value={"authenticated": True, "is_admin": True, "is_superadmin": True},
    )
    assert result.errors is None
    assert result.data == {
        "getUser": {
            "name": "Bob Smith",
            "email": "bob@example.com",
            "phone": "+9876543210",
            "ssn": "123-45-6789",
        }
    }


def test_field_authorization_with_owner_check():
    """Test field authorization that checks ownership."""

    @fraiseql.type
    class UserProfile:
        id: int
        name: str
        private_notes_value: str = ""

        @authorize_field(
            lambda info, root: (
                info.context.get("user_id") == root.id or info.context.get("is_admin", False)
            )
        )
        @field
        def private_notes(self) -> str:
            return self.private_notes_value

    @query
    def user_profile(info, user_id: int) -> UserProfile:
        # Simulate fetching user profile
        return UserProfile(
            id=user_id,
            name=f"User {user_id}",
            private_notes_value=f"Private notes for user {user_id}",
        )

    schema = build_fraiseql_schema(query_types=[user_profile])

    query_str = """
    query {
        userProfile(userId: 123) {
            id
            name
            privateNotes
        }
    }
    """

    # Test owner access
    result = graphql_sync(schema, query_str, context_value={"user_id": 123})
    assert result.errors is None
    assert result.data["userProfile"]["privateNotes"] == "Private notes for user 123"

    # Test non-owner access
    result = graphql_sync(schema, query_str, context_value={"user_id": 456})
    assert result.errors is not None
    assert result.data["userProfile"]["privateNotes"] is None

    # Test admin access
    result = graphql_sync(schema, query_str, context_value={"user_id": 789, "is_admin": True})
    assert result.errors is None
    assert result.data["userProfile"]["privateNotes"] == "Private notes for user 123"


def test_field_authorization_async():
    """Test field authorization with async fields."""
    import asyncio

    from graphql import graphql

    @fraiseql.type
    class AsyncData:
        id: int
        secret_value: str = ""

        @authorize_field(lambda info: info.context.get("has_access", False))
        @field
        async def secret(self) -> str:
            # Simulate async operation
            await asyncio.sleep(0.001)
            return self.secret_value

    @query
    async def async_data(info) -> AsyncData:
        return AsyncData(id=1, secret_value="async secret data")

    schema = build_fraiseql_schema(query_types=[async_data])

    query_str = """
    query {
        asyncData {
            id
            secret
        }
    }
    """

    # Test with access
    result = asyncio.run(graphql(schema, query_str, context_value={"has_access": True}))
    assert result.errors is None
    assert result.data == {"asyncData": {"id": 1, "secret": "async secret data"}}

    # Test without access
    result = asyncio.run(graphql(schema, query_str, context_value={"has_access": False}))
    assert result.errors is not None
    assert result.data == {"asyncData": {"id": 1, "secret": None}}
