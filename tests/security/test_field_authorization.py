"""Tests for field-level authorization in FraiseQL."""

import pytest

from fraiseql import fraise_type, query
from fraiseql.decorators import field
from fraiseql.gql.schema_builder import build_fraiseql_schema
from fraiseql.security.field_auth import FieldAuthorizationError, authorize_field


class TestFieldAuthorization:
    """Test field-level authorization functionality."""

    def test_authorize_field_decorator_allows_access(self):
        """Test that authorize_field allows access when permission check passes."""

        @fraise_type
        class User:
            name: str

            @field
            @authorize_field(lambda info: info.context.get("is_admin", False))
            def email(self) -> str:
                return "admin@example.com"

        # Add a query to make schema valid
        @query
        def get_user(info) -> User:
            return User(name="Test User")

        build_fraiseql_schema(query_types=[get_user])

        # Create mock info with admin context
        class MockInfo:
            context = {"is_admin": True}
            field_name = "email"

        user = User(name="Test User")
        result = user.email(MockInfo())
        assert result == "admin@example.com"

    def test_authorize_field_decorator_denies_access(self):
        """Test that authorize_field denies access when permission check fails."""

        @fraise_type
        class User:
            name: str

            @field
            @authorize_field(lambda info: info.context.get("is_admin", False))
            def email(self) -> str:
                return "admin@example.com"

        # Add a query to make schema valid
        @query
        def get_user(info) -> User:
            return User(name="Test User")

        build_fraiseql_schema(query_types=[get_user])

        # Create mock info without admin context
        class MockInfo:
            context = {"is_admin": False}
            field_name = "email"

        user = User(name="Test User")
        with pytest.raises(FieldAuthorizationError):
            user.email(MockInfo())

    def test_authorize_field_with_custom_error_message(self):
        """Test authorize_field with custom error message."""

        @fraise_type
        class User:
            name: str

            @field
            @authorize_field(
                lambda info: info.context.get("is_admin", False),
                error_message="Admin access required",
            )
            def email(self) -> str:
                return "admin@example.com"

        # Create mock info without admin context
        class MockInfo:
            context = {"is_admin": False}
            field_name = "email"

        user = User(name="Test User")
        with pytest.raises(FieldAuthorizationError, match="Admin access required"):
            user.email(MockInfo())

    def test_authorize_field_with_multiple_permissions(self):
        """Test field authorization with multiple permission checks."""

        def is_admin(info):
            return info.context.get("is_admin", False)

        def is_owner(info):
            return info.context.get("user_id") == info.context.get("resource_owner_id")

        @fraise_type
        class User:
            name: str
            id: int

            @field
            @authorize_field(lambda info: is_admin(info) or is_owner(info))
            def email(self) -> str:
                return "user@example.com"

        # Test admin access
        class AdminInfo:
            context = {"is_admin": True, "user_id": 1, "resource_owner_id": 2}
            field_name = "email"

        user = User(name="Test", id=2)
        assert user.email(AdminInfo()) == "user@example.com"

        # Test owner access
        class OwnerInfo:
            context = {"is_admin": False, "user_id": 2, "resource_owner_id": 2}
            field_name = "email"

        assert user.email(OwnerInfo()) == "user@example.com"

        # Test denied access
        class DeniedInfo:
            context = {"is_admin": False, "user_id": 3, "resource_owner_id": 2}
            field_name = "email"

        with pytest.raises(FieldAuthorizationError):
            user.email(DeniedInfo())

    def test_authorize_field_async_permission_check(self):
        """Test authorize_field with async permission check."""
        import asyncio

        async def async_permission_check(info):
            # Simulate async permission check
            await asyncio.sleep(0.001)
            return info.context.get("is_admin", False)

        @fraise_type
        class User:
            name: str

            @field
            @authorize_field(async_permission_check)
            async def email(self) -> str:
                return "admin@example.com"

        # Test allowed access
        class AdminInfo:
            context = {"is_admin": True}
            field_name = "email"

        user = User(name="Test")
        result = asyncio.run(user.email(AdminInfo()))
        assert result == "admin@example.com"

        # Test denied access
        class NonAdminInfo:
            context = {"is_admin": False}
            field_name = "email"

        with pytest.raises(FieldAuthorizationError):
            asyncio.run(user.email(NonAdminInfo()))

    def test_field_auth_with_arguments(self):
        """Test field authorization that considers field arguments."""

        def can_view_details(info, include_private: bool = False):
            if include_private:
                return info.context.get("is_admin", False)
            return True

        @fraise_type
        class User:
            name: str

            @field
            @authorize_field(can_view_details)
            def details(self, include_private: bool = False) -> str:
                if include_private:
                    return "Name: Test, Email: secret@example.com"
                return "Name: Test"

        # Test public access
        class PublicInfo:
            context = {"is_admin": False}
            field_name = "details"

        user = User(name="Test")
        assert user.details(PublicInfo(), include_private=False) == "Name: Test"

        # Test private access denied
        with pytest.raises(FieldAuthorizationError):
            user.details(PublicInfo(), include_private=True)

        # Test private access allowed
        class AdminInfo:
            context = {"is_admin": True}
            field_name = "details"

        result = user.details(AdminInfo(), include_private=True)
        assert result == "Name: Test, Email: secret@example.com"

    def test_field_auth_integration_with_graphql(self):
        """Test field authorization in actual GraphQL execution."""
        from graphql import graphql_sync

        @fraise_type
        class SecureData:
            public_info: str

            @field
            @authorize_field(lambda info: info.context.get("authenticated", False))
            def private_info(self) -> str:
                return "secret data"

        @query
        def secure_data(info) -> SecureData:
            return SecureData(public_info="public data")

        schema = build_fraiseql_schema(query_types=[secure_data])

        # Test authenticated access
        query_str = """
        query {
            secureData {
                publicInfo
                privateInfo
            }
        }
        """

        result = graphql_sync(
            schema,
            query_str,
            context_value={"authenticated": True},
        )

        assert result.errors is None
        assert result.data == {
            "secureData": {
                "publicInfo": "public data",
                "privateInfo": "secret data",
            },
        }

        # Test unauthenticated access
        result = graphql_sync(
            schema,
            query_str,
            context_value={"authenticated": False},
        )

        assert result.errors is not None
        assert len(result.errors) == 1
        assert "Not authorized to access field" in str(result.errors[0])
        # Public field should still be accessible
        assert result.data == {
            "secureData": {
                "publicInfo": "public data",
                "privateInfo": None,
            },
        }
