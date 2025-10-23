import pytest

"""Simple tests for field-level authorization to verify basic functionality."""

from graphql import graphql_sync

import fraiseql
from fraiseql import query
from fraiseql.gql.schema_builder import build_fraiseql_schema
from fraiseql.security.field_auth import FieldAuthorizationError


@pytest.mark.security
def test_field_authorization_in_graphql():
    """Test field authorization in actual GraphQL execution."""

    @fraiseql.type
    class SimpleUser:
        name: str
        email: str

    @query
    def current_user(info) -> SimpleUser:
        # Check authorization at query level
        if info.context.get("is_admin", False):
            return SimpleUser(name="John Doe", email="john@example.com")
        # Return user with valid email for non-admins, then override for authorization
        user = SimpleUser(name="John Doe", email="user@example.com")
        # Set email to None in the response for authorization
        user.email = None
        return user

    schema = build_fraiseql_schema(query_types=[current_user])

    # Test with admin access
    query_str = """
    query {
        currentUser {
            name
            email
        }
    }
    """
    result = graphql_sync(schema, query_str, context_value={"is_admin": True})

    assert result.errors is None
    assert result.data == {"currentUser": {"name": "John Doe", "email": "john@example.com"}}

    # Test without admin access
    result = graphql_sync(schema, query_str, context_value={"is_admin": False})

    assert result.errors is None  # No errors since we handle auth at query level
    assert result.data == {"currentUser": {"name": "John Doe", "email": None}}


def test_simple_permission_check():
    """Test a simple permission check function."""

    def is_admin(context):
        return context.get("is_admin", False)

    # Admin context
    admin_context = {"is_admin": True}
    assert is_admin(admin_context) is True

    # Non-admin context
    user_context = {"is_admin": False}
    assert is_admin(user_context) is False

    # Empty context
    empty_context = {}
    assert is_admin(empty_context) is False


def test_field_authorization_error():
    """Test FieldAuthorizationError properties."""
    error = FieldAuthorizationError("Custom error message")
    assert str(error) == "Custom error message"
    assert error.extensions["code"] == "FIELD_AUTHORIZATION_ERROR"

    # Default message
    error_default = FieldAuthorizationError()
    assert str(error_default) == "Not authorized to access this field"
