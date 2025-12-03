"""Extended tests for auth decorators to improve coverage."""

from unittest.mock import Mock

import pytest
from graphql import GraphQLError, GraphQLResolveInfo

from fraiseql.auth.base import UserContext
from fraiseql.auth.decorators import (
    requires_any_permission,
    requires_any_role,
    requires_auth,
    requires_permission,
    requires_role,
)


class MockUserContext(UserContext):
    """Mock UserContext for testing."""

    def __init__(
        self,
        user_id: str = "test_user",
        permissions: list[str] | None = None,
        roles: list[str] | None = None,
    ):
        super().__init__(user_id=user_id)
        self._permissions = permissions or []
        self._roles = roles or []

    def has_permission(self, permission: str) -> bool:
        """Check if user has specific permission."""
        return permission in self._permissions

    def has_role(self, role: str) -> bool:
        """Check if user has specific role."""
        return role in self._roles

    def has_any_permission(self, permissions: list[str]) -> bool:
        """Check if user has any of the specified permissions."""
        return any(perm in self._permissions for perm in permissions)

    def has_any_role(self, roles: list[str]) -> bool:
        """Check if user has any of the specified roles."""
        return any(role in self._roles for role in roles)


class TestRequiresAuth:
    """Test requires_auth decorator."""

    def create_mock_info(self, user=None) -> None:
        """Create mock GraphQL resolve info."""
        mock_info = Mock(spec=GraphQLResolveInfo)
        mock_info.context = {"user": user} if user else {}
        return mock_info

    @pytest.mark.asyncio
    async def test_requires_auth_success(self) -> None:
        """Test successful authentication."""
        user = MockUserContext("test_user")
        mock_info = self.create_mock_info(user)

        @requires_auth
        @pytest.mark.asyncio
        async def test_resolver(info) -> None:
            return {"success": True}

        result = await test_resolver(mock_info)
        assert result == {"success": True}

    @pytest.mark.asyncio
    async def test_requires_auth_no_user(self) -> None:
        """Test authentication failure with no user."""
        mock_info = self.create_mock_info()

        @requires_auth
        @pytest.mark.asyncio
        async def test_resolver(info) -> None:
            return {"success": True}

        with pytest.raises(GraphQLError, match="Authentication required"):
            await test_resolver(mock_info)

    @pytest.mark.asyncio
    async def test_requires_auth_invalid_user_type(self) -> None:
        """Test authentication failure with invalid user type."""
        mock_info = self.create_mock_info({"not": "a_user_context"})

        @requires_auth
        @pytest.mark.asyncio
        async def test_resolver(info) -> None:
            return {"success": True}

        with pytest.raises(GraphQLError, match="Authentication required"):
            await test_resolver(mock_info)

    @pytest.mark.asyncio
    async def test_requires_auth_no_info(self) -> None:
        """Test authentication with no info argument."""

        @requires_auth
        async def test_resolver() -> None:
            return {"success": True}

        with pytest.raises(ValueError, match="GraphQL resolver must have info"):
            await test_resolver()

    @pytest.mark.asyncio
    async def test_requires_auth_invalid_info_type(self) -> None:
        """Test authentication with invalid info type."""

        @requires_auth
        async def test_resolver(info) -> None:
            return {"success": True}

        with pytest.raises(TypeError, match="First argument must be GraphQLResolveInfo"):
            await test_resolver("not_info")

    @pytest.mark.asyncio
    async def test_requires_auth_with_kwargs(self) -> None:
        """Test authentication with info passed as kwarg."""
        user = MockUserContext("test_user")
        mock_info = self.create_mock_info(user)

        @requires_auth
        @pytest.mark.asyncio
        async def test_resolver(info=None, other_arg="test") -> None:
            return {"success": True, "arg": other_arg}

        result = await test_resolver(info=mock_info, other_arg="passed")
        assert result == {"success": True, "arg": "passed"}

    @pytest.mark.asyncio
    async def test_requires_auth_preserves_function_signature(self) -> None:
        """Test that decorator preserves function metadata."""

        @requires_auth
        async def documented_resolver(info, param: str) -> None:
            """This is a documented resolver."""
            return {"param": param}

        assert documented_resolver.__name__ == "documented_resolver"
        assert documented_resolver.__doc__ == "This is a documented resolver."


class TestRequiresPermission:
    """Test requires_permission decorator."""

    def create_mock_info(self, user=None) -> None:
        """Create mock GraphQL resolve info."""
        mock_info = Mock(spec=GraphQLResolveInfo)
        mock_info.context = {"user": user} if user else {}
        return mock_info

    @pytest.mark.asyncio
    async def test_requires_permission_success(self) -> None:
        """Test successful permission check."""
        user = MockUserContext("test_user", permissions=["users:read", "users:write"])
        mock_info = self.create_mock_info(user)

        @requires_permission("users:read")
        @pytest.mark.asyncio
        async def test_resolver(info) -> None:
            return {"success": True}

        result = await test_resolver(mock_info)
        assert result == {"success": True}

    @pytest.mark.asyncio
    async def test_requires_permission_no_user(self) -> None:
        """Test permission failure with no user."""
        mock_info = self.create_mock_info()

        @requires_permission("users:read")
        @pytest.mark.asyncio
        async def test_resolver(info) -> None:
            return {"success": True}

        with pytest.raises(GraphQLError, match="Authentication required"):
            await test_resolver(mock_info)

    @pytest.mark.asyncio
    async def test_requires_permission_insufficient_permissions(self) -> None:
        """Test permission failure with insufficient permissions."""
        user = MockUserContext("test_user", permissions=["users:read"])
        mock_info = self.create_mock_info(user)

        @requires_permission("users:write")
        @pytest.mark.asyncio
        async def test_resolver(info) -> None:
            return {"success": True}

        with pytest.raises(GraphQLError, match="Permission 'users:write' required") as exc_info:
            await test_resolver(mock_info)

        assert exc_info.value.extensions["code"] == "FORBIDDEN"
        assert exc_info.value.extensions["required_permission"] == "users:write"

    @pytest.mark.asyncio
    async def test_requires_permission_no_info(self) -> None:
        """Test permission decorator with no info argument."""

        @requires_permission("users:read")
        async def test_resolver() -> None:
            return {"success": True}

        with pytest.raises(ValueError, match="GraphQL resolver must have info"):
            await test_resolver()

    @pytest.mark.asyncio
    async def test_requires_permission_invalid_info_type(self) -> None:
        """Test permission decorator with invalid info type."""

        @requires_permission("users:read")
        async def test_resolver(info) -> None:
            return {"success": True}

        with pytest.raises(TypeError, match="First argument must be GraphQLResolveInfo"):
            await test_resolver("not_info")


class TestRequiresRole:
    """Test requires_role decorator."""

    def create_mock_info(self, user=None) -> None:
        """Create mock GraphQL resolve info."""
        mock_info = Mock(spec=GraphQLResolveInfo)
        mock_info.context = {"user": user} if user else {}
        return mock_info

    @pytest.mark.asyncio
    async def test_requires_role_success(self) -> None:
        """Test successful role check."""
        user = MockUserContext("test_user", roles=["admin", "user"])
        mock_info = self.create_mock_info(user)

        @requires_role("admin")
        @pytest.mark.asyncio
        async def test_resolver(info) -> None:
            return {"success": True}

        result = await test_resolver(mock_info)
        assert result == {"success": True}

    @pytest.mark.asyncio
    async def test_requires_role_no_user(self) -> None:
        """Test role failure with no user."""
        mock_info = self.create_mock_info()

        @requires_role("admin")
        @pytest.mark.asyncio
        async def test_resolver(info) -> None:
            return {"success": True}

        with pytest.raises(GraphQLError, match="Authentication required"):
            await test_resolver(mock_info)

    @pytest.mark.asyncio
    async def test_requires_role_insufficient_role(self) -> None:
        """Test role failure with insufficient role."""
        user = MockUserContext("test_user", roles=["user"])
        mock_info = self.create_mock_info(user)

        @requires_role("admin")
        @pytest.mark.asyncio
        async def test_resolver(info) -> None:
            return {"success": True}

        with pytest.raises(GraphQLError, match="Role 'admin' required") as exc_info:
            await test_resolver(mock_info)

        assert exc_info.value.extensions["code"] == "FORBIDDEN"
        assert exc_info.value.extensions["required_role"] == "admin"

    @pytest.mark.asyncio
    async def test_requires_role_no_info(self) -> None:
        """Test role decorator with no info argument."""

        @requires_role("admin")
        async def test_resolver() -> None:
            return {"success": True}

        with pytest.raises(ValueError, match="GraphQL resolver must have info"):
            await test_resolver()

    @pytest.mark.asyncio
    async def test_requires_role_invalid_info_type(self) -> None:
        """Test role decorator with invalid info type."""

        @requires_role("admin")
        async def test_resolver(info) -> None:
            return {"success": True}

        with pytest.raises(TypeError, match="First argument must be GraphQLResolveInfo"):
            await test_resolver("not_info")


class TestRequiresAnyPermission:
    """Test requires_any_permission decorator."""

    def create_mock_info(self, user=None) -> None:
        """Create mock GraphQL resolve info."""
        mock_info = Mock(spec=GraphQLResolveInfo)
        mock_info.context = {"user": user} if user else {}
        return mock_info

    @pytest.mark.asyncio
    async def test_requires_any_permission_success_first(self) -> None:
        """Test successful check with first permission."""
        user = MockUserContext("test_user", permissions=["users:read", "posts:write"])
        mock_info = self.create_mock_info(user)

        @requires_any_permission("users:read", "admin:all")
        @pytest.mark.asyncio
        async def test_resolver(info) -> None:
            return {"success": True}

        result = await test_resolver(mock_info)
        assert result == {"success": True}

    @pytest.mark.asyncio
    async def test_requires_any_permission_success_second(self) -> None:
        """Test successful check with second permission."""
        user = MockUserContext("test_user", permissions=["posts:write", "admin:all"])
        mock_info = self.create_mock_info(user)

        @requires_any_permission("users:read", "admin:all")
        @pytest.mark.asyncio
        async def test_resolver(info) -> None:
            return {"success": True}

        result = await test_resolver(mock_info)
        assert result == {"success": True}

    @pytest.mark.asyncio
    async def test_requires_any_permission_no_user(self) -> None:
        """Test failure with no user."""
        mock_info = self.create_mock_info()

        @requires_any_permission("users:read", "admin:all")
        @pytest.mark.asyncio
        async def test_resolver(info) -> None:
            return {"success": True}

        with pytest.raises(GraphQLError, match="Authentication required"):
            await test_resolver(mock_info)

    @pytest.mark.asyncio
    async def test_requires_any_permission_no_matching_permissions(self) -> None:
        """Test failure with no matching permissions."""
        user = MockUserContext("test_user", permissions=["posts:write"])
        mock_info = self.create_mock_info(user)

        @requires_any_permission("users:read", "admin:all")
        @pytest.mark.asyncio
        async def test_resolver(info) -> None:
            return {"success": True}

        with pytest.raises(GraphQLError, match="One of these permissions required") as exc_info:
            await test_resolver(mock_info)

        assert exc_info.value.extensions["code"] == "FORBIDDEN"
        assert exc_info.value.extensions["required_permissions"] == ["users:read", "admin:all"]

    @pytest.mark.asyncio
    async def test_requires_any_permission_single_permission(self) -> None:
        """Test with single permission argument."""
        user = MockUserContext("test_user", permissions=["users:read"])
        mock_info = self.create_mock_info(user)

        @requires_any_permission("users:read")
        @pytest.mark.asyncio
        async def test_resolver(info) -> None:
            return {"success": True}

        result = await test_resolver(mock_info)
        assert result == {"success": True}

    @pytest.mark.asyncio
    async def test_requires_any_permission_no_info(self) -> None:
        """Test with no info argument."""

        @requires_any_permission("users:read", "admin:all")
        async def test_resolver() -> None:
            return {"success": True}

        with pytest.raises(ValueError, match="GraphQL resolver must have info"):
            await test_resolver()


class TestRequiresAnyRole:
    """Test requires_any_role decorator."""

    def create_mock_info(self, user=None) -> None:
        """Create mock GraphQL resolve info."""
        mock_info = Mock(spec=GraphQLResolveInfo)
        mock_info.context = {"user": user} if user else {}
        return mock_info

    @pytest.mark.asyncio
    async def test_requires_any_role_success_first(self) -> None:
        """Test successful check with first role."""
        user = MockUserContext("test_user", roles=["admin", "user"])
        mock_info = self.create_mock_info(user)

        @requires_any_role("admin", "moderator")
        @pytest.mark.asyncio
        async def test_resolver(info) -> None:
            return {"success": True}

        result = await test_resolver(mock_info)
        assert result == {"success": True}

    @pytest.mark.asyncio
    async def test_requires_any_role_success_second(self) -> None:
        """Test successful check with second role."""
        user = MockUserContext("test_user", roles=["user", "moderator"])
        mock_info = self.create_mock_info(user)

        @requires_any_role("admin", "moderator")
        @pytest.mark.asyncio
        async def test_resolver(info) -> None:
            return {"success": True}

        result = await test_resolver(mock_info)
        assert result == {"success": True}

    @pytest.mark.asyncio
    async def test_requires_any_role_no_user(self) -> None:
        """Test failure with no user."""
        mock_info = self.create_mock_info()

        @requires_any_role("admin", "moderator")
        @pytest.mark.asyncio
        async def test_resolver(info) -> None:
            return {"success": True}

        with pytest.raises(GraphQLError, match="Authentication required"):
            await test_resolver(mock_info)

    @pytest.mark.asyncio
    async def test_requires_any_role_no_matching_roles(self) -> None:
        """Test failure with no matching roles."""
        user = MockUserContext("test_user", roles=["user"])
        mock_info = self.create_mock_info(user)

        @requires_any_role("admin", "moderator")
        @pytest.mark.asyncio
        async def test_resolver(info) -> None:
            return {"success": True}

        with pytest.raises(GraphQLError, match="One of these roles required") as exc_info:
            await test_resolver(mock_info)

        assert exc_info.value.extensions["code"] == "FORBIDDEN"
        assert exc_info.value.extensions["required_roles"] == ["admin", "moderator"]

    @pytest.mark.asyncio
    async def test_requires_any_role_single_role(self) -> None:
        """Test with single role argument."""
        user = MockUserContext("test_user", roles=["admin"])
        mock_info = self.create_mock_info(user)

        @requires_any_role("admin")
        @pytest.mark.asyncio
        async def test_resolver(info) -> None:
            return {"success": True}

        result = await test_resolver(mock_info)
        assert result == {"success": True}

    @pytest.mark.asyncio
    async def test_requires_any_role_no_info(self) -> None:
        """Test with no info argument."""

        @requires_any_role("admin", "moderator")
        async def test_resolver() -> None:
            return {"success": True}

        with pytest.raises(ValueError, match="GraphQL resolver must have info"):
            await test_resolver()


class TestDecoratorCombinations:
    """Test combinations of decorators."""

    def create_mock_info(self, user=None) -> None:
        """Create mock GraphQL resolve info."""
        mock_info = Mock(spec=GraphQLResolveInfo)
        mock_info.context = {"user": user} if user else {}
        return mock_info

    @pytest.mark.asyncio
    async def test_multiple_decorators_success(self) -> None:
        """Test combining multiple auth decorators."""
        user = MockUserContext("test_user", permissions=["users:write"], roles=["admin"])
        mock_info = self.create_mock_info(user)

        @requires_auth
        @requires_permission("users:write")
        @requires_role("admin")
        @pytest.mark.asyncio
        async def test_resolver(info) -> None:
            return {"success": True}

        result = await test_resolver(mock_info)
        assert result == {"success": True}

    @pytest.mark.asyncio
    async def test_multiple_decorators_auth_failure(self) -> None:
        """Test multiple decorators with auth failure."""
        mock_info = self.create_mock_info()

        @requires_auth
        @requires_permission("users:write")
        @pytest.mark.asyncio
        async def test_resolver(info) -> None:
            return {"success": True}

        with pytest.raises(GraphQLError, match="Authentication required"):
            await test_resolver(mock_info)

    @pytest.mark.asyncio
    async def test_multiple_decorators_permission_failure(self) -> None:
        """Test multiple decorators with permission failure."""
        user = MockUserContext("test_user", permissions=["users:read"])  # Missing write,
        mock_info = self.create_mock_info(user)

        @requires_auth
        @requires_permission("users:write")
        @pytest.mark.asyncio
        async def test_resolver(info) -> None:
            return {"success": True}

        with pytest.raises(GraphQLError, match="Permission 'users:write' required"):
            await test_resolver(mock_info)

    @pytest.mark.asyncio
    async def test_multiple_decorators_role_failure(self) -> None:
        """Test multiple decorators with role failure."""
        user = MockUserContext(
            "test_user", permissions=["users:write"], roles=["user"]
        )  # Missing admin
        mock_info = self.create_mock_info(user)

        @requires_auth
        @requires_permission("users:write")
        @requires_role("admin")
        @pytest.mark.asyncio
        async def test_resolver(info) -> None:
            return {"success": True}

        with pytest.raises(GraphQLError, match="Role 'admin' required"):
            await test_resolver(mock_info)


class TestEdgeCases:
    """Test edge cases and error conditions."""

    def create_mock_info(self, user=None) -> None:
        """Create mock GraphQL resolve info."""
        mock_info = Mock(spec=GraphQLResolveInfo)
        mock_info.context = {"user": user} if user else {}
        return mock_info

    @pytest.mark.asyncio
    async def test_decorator_with_resolver_arguments(self) -> None:
        """Test decorators work with resolver arguments."""
        user = MockUserContext("test_user", permissions=["users:read"])
        mock_info = self.create_mock_info(user)

        @requires_permission("users:read")
        @pytest.mark.asyncio
        async def test_resolver(info, user_id: str, filters: dict | None = None) -> None:
            return {"user_id": user_id, "filters": filters}

        result = await test_resolver(mock_info, "123", {"active": True})
        assert result == {"user_id": "123", "filters": {"active": True}}

    @pytest.mark.asyncio
    async def test_decorator_preserves_exception_from_resolver(self) -> None:
        """Test that decorators don't swallow resolver exceptions."""
        user = MockUserContext("test_user")
        mock_info = self.create_mock_info(user)

        @requires_auth
        async def failing_resolver(info) -> None:
            raise ValueError("Resolver failed")

        with pytest.raises(ValueError, match="Resolver failed"):
            await failing_resolver(mock_info)

    @pytest.mark.asyncio
    async def test_empty_permissions_list(self) -> None:
        """Test requires_any_permission with empty permissions."""
        user = MockUserContext("test_user", permissions=[])
        mock_info = self.create_mock_info(user)

        @requires_any_permission()
        @pytest.mark.asyncio
        async def test_resolver(info) -> None:
            return {"success": True}

        # Empty permissions list still requires at least one permission, so should fail
        with pytest.raises(GraphQLError, match="One of these permissions required"):
            await test_resolver(mock_info)

    @pytest.mark.asyncio
    async def test_empty_roles_list(self) -> None:
        """Test requires_any_role with empty roles."""
        user = MockUserContext("test_user", roles=[])
        mock_info = self.create_mock_info(user)

        @requires_any_role()
        @pytest.mark.asyncio
        async def test_resolver(info) -> None:
            return {"success": True}

        # Empty roles list still requires at least one role, so should fail
        with pytest.raises(GraphQLError, match="One of these roles required"):
            await test_resolver(mock_info)

    @pytest.mark.asyncio
    async def test_user_context_none_in_context(self) -> None:
        """Test when user is explicitly None in context."""
        mock_info = Mock(spec=GraphQLResolveInfo)
        mock_info.context = {"user": None}

        @requires_auth
        @pytest.mark.asyncio
        async def test_resolver(info) -> None:
            return {"success": True}

        with pytest.raises(GraphQLError, match="Authentication required"):
            await test_resolver(mock_info)

    @pytest.mark.asyncio
    async def test_complex_permission_names(self) -> None:
        """Test with complex permission names."""
        user = MockUserContext("test_user", permissions=["org:123:users:write", "global:admin"])
        mock_info = self.create_mock_info(user)

        @requires_permission("org:123:users:write")
        @pytest.mark.asyncio
        async def test_resolver(info) -> None:
            return {"success": True}

        result = await test_resolver(mock_info)
        assert result == {"success": True}

    @pytest.mark.asyncio
    async def test_complex_role_names(self) -> None:
        """Test with complex role names."""
        user = MockUserContext("test_user", roles=["org-admin", "super_user"])
        mock_info = self.create_mock_info(user)

        @requires_role("org-admin")
        @pytest.mark.asyncio
        async def test_resolver(info) -> None:
            return {"success": True}

        result = await test_resolver(mock_info)
        assert result == {"success": True}
