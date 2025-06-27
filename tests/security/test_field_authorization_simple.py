"""Simple tests for field-level authorization to verify basic functionality."""

from graphql import graphql_sync

from fraiseql import fraise_type, query
from fraiseql.decorators import field
from fraiseql.gql.schema_builder import build_fraiseql_schema
from fraiseql.security.field_auth import FieldAuthorizationError


def test_field_authorization_in_graphql():
    """Test field authorization in actual GraphQL execution."""

    @fraise_type
    class User:
        name: str
        email_value: str = "user@example.com"

        @field
        def email(self) -> str:
            """Email field that requires admin access."""
            # Get info from context
            info = self._graphql_info  # This would be set by the framework
            if not info.context.get("is_admin", False):
                raise FieldAuthorizationError("Admin access required to view email")
            return self.email_value

    @query
    def current_user(info) -> User:
        user = User(name="John Doe", email_value="john@example.com")
        # Store info for field resolver
        user._graphql_info = info
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

    result = graphql_sync(
        schema,
        query_str,
        context_value={"is_admin": True},
    )

    assert result.errors is None
    assert result.data == {
        "currentUser": {
            "name": "John Doe",
            "email": "john@example.com",
        },
    }

    # Test without admin access
    result = graphql_sync(
        schema,
        query_str,
        context_value={"is_admin": False},
    )

    assert result.errors is not None
    assert len(result.errors) == 1
    assert "Admin access required" in str(result.errors[0])
    assert result.data == {
        "currentUser": {
            "name": "John Doe",
            "email": None,
        },
    }


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
