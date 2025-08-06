"""Tests for complex field authorization scenarios."""

import asyncio
from unittest.mock import MagicMock

import pytest
from graphql import GraphQLResolveInfo

import fraiseql
from fraiseql import field
from fraiseql.security.field_auth import (
    FieldAuthorizationError,
    any_permission,
    authorize_field,
    combine_permissions,
)


class TestComplexFieldAuthorization:
    """Test complex field authorization scenarios."""

    def test_nested_permission_checks(self):
        """Test deeply nested permission checks."""

        # Create nested permission checks
        # Permission checks receive (info, *args, **kwargs) but can ignore extra args
        def is_authenticated(info, *args, **kwargs):
            return info.context.get("user") is not None

        def is_admin(info, *args, **kwargs):
            return info.context.get("user", {}).get("role") == "admin"

        def is_owner(info, *args, **kwargs):
            return info.context.get("user", {}).get("""id""") == info.context.get(
                "resource_owner_id"
            )

        # Combine: must be authenticated AND (admin OR owner)
        complex_check = combine_permissions(is_authenticated, any_permission(is_admin, is_owner))

        @fraiseql.type
        class SecureResource:
            id: int
            public_data: str

            @field
            @authorize_field(
                complex_check, error_message="Must be authenticated and either admin or owner"
            )
            def sensitive_data(self, info) -> str:
                return "secret"

        # Test various scenarios
        info = MagicMock(spec=GraphQLResolveInfo)
        resource = SecureResource(id=1, public_data="public")

        # For testing, we need to simulate how GraphQL would call this
        # The field decorator expects the method to be unbound
        resolver = SecureResource.sensitive_data

        # Not authenticated
        info.context = {}
        with pytest.raises(FieldAuthorizationError) as exc:
            resolver(resource, info)
        assert "Must be authenticated and either admin or owner" in str(exc.value)

        # Authenticated but not admin or owner
        info.context = {"user": {"id": 999, "role": "user"}, "resource_owner_id": 1}
        with pytest.raises(FieldAuthorizationError):
            resolver(resource, info)

        # Authenticated and admin
        info.context = {"user": {"id": 999, "role": "admin"}, "resource_owner_id": 1}
        assert resolver(resource, info) == "secret"

        # Authenticated and owner
        info.context = {"user": {"id": 1, "role": "user"}, "resource_owner_id": 1}
        assert resolver(resource, info) == "secret"

    @pytest.mark.asyncio
    async def test_async_permission_with_database_check(self):
        """Test async permissions that query database."""

        # Mock database check
        async def has_permission_in_db(info, resource_id: int, permission: str) -> bool:
            # Simulate DB query
            await asyncio.sleep(0.01)
            db_permissions = info.context.get("db_permissions", {})
            return db_permissions.get(f"{resource_id}:{permission}", False)

        # Create async permission check
        async def can_view_financial_data(info, *args, **kwargs) -> bool:
            user = info.context.get("user")
            if not user:
                return False

            # Check special permission in database
            return await has_permission_in_db(info, user["id"], "view_financial")

        @fraiseql.type
        class Company:
            name: str

            @field
            @authorize_field(can_view_financial_data)
            async def financial_report(self, info) -> dict:
                return {"revenue": 1000000, "profit": 100000}

        company = Company(name="Test Corp")
        info = MagicMock(spec=GraphQLResolveInfo)

        # Get the unbound method for testing
        resolver = Company.financial_report

        # No user
        info.context = {}
        with pytest.raises(FieldAuthorizationError):
            await resolver(company, info)

        # User without permission
        info.context = {"user": {"id": 1}, "db_permissions": {}}
        with pytest.raises(FieldAuthorizationError):
            await resolver(company, info)

        # User with permission
        info.context = {"user": {"id": 1}, "db_permissions": {"1:view_financial": True}}
        result = await resolver(company, info)
        assert result["revenue"] == 1000000

    def test_permission_with_field_arguments(self):
        """Test permissions that depend on field arguments."""

        def can_access_user_data(info, *args, **kwargs) -> bool:
            # Extract user_id from kwargs (field arguments)
            user_id = kwargs.get("user_id")
            if user_id is None and args:
                # If not in kwargs, might be in positional args
                # Skip the root object (first arg) and get the user_id
                user_id = args[1] if len(args) > 1 else None

            current_user = info.context.get("user")
            if not current_user:
                return False

            # Admin can access anyone
            if current_user.get("role") == "admin":
                return True

            # Users can only access their own data
            return current_user.get("id") == user_id

        @fraiseql.type
        class Query:
            @field
            @authorize_field(can_access_user_data)
            def user_profile(self, info, user_id: int) -> dict:
                return {"id": user_id, "email": f"user{user_id}@example.com"}

        query = Query()
        info = MagicMock(spec=GraphQLResolveInfo)
        # Get the unbound method
        resolver = Query.user_profile

        # Not authenticated
        info.context = {}
        with pytest.raises(FieldAuthorizationError):
            resolver(query, info, user_id=1)

        # User accessing own profile
        info.context = {"user": {"id": 1, "role": "user"}}
        result = resolver(query, info, user_id=1)
        assert result["id"] == 1

        # User accessing other's profile
        with pytest.raises(FieldAuthorizationError):
            resolver(query, info, user_id=2)

        # Admin accessing any profile
        info.context = {"user": {"id": 999, "role": "admin"}}
        result = resolver(query, info, user_id=2)
        assert result["id"] == 2

    def test_rate_limiting_permission(self):
        """Test permission check with rate limiting."""

        class RateLimiter:
            def __init__(self, max_requests: int = 10):
                self.requests = {}
                self.max_requests = max_requests

            def check_rate_limit(self, info, *args, **kwargs) -> bool:
                user = info.context.get("user")
                if not user:
                    return False

                user_id = user["id"]
                current_count = self.requests.get(user_id, 0)

                if current_count >= self.max_requests:
                    return False

                self.requests[user_id] = current_count + 1
                return True

        rate_limiter = RateLimiter(max_requests=2)

        @fraiseql.type
        class ExpensiveQuery:
            @field
            @authorize_field(rate_limiter.check_rate_limit, error_message="Rate limit exceeded")
            def expensive_operation(self, info) -> str:
                return "result"

        query = ExpensiveQuery()
        info = MagicMock(spec=GraphQLResolveInfo)
        info.context = {"user": {"id": 1}}
        resolver = ExpensiveQuery.expensive_operation

        # First two requests succeed
        assert resolver(query, info) == "result"
        assert resolver(query, info) == "result"

        # Third request fails
        with pytest.raises(FieldAuthorizationError) as exc:
            resolver(query, info)
        assert "Rate limit exceeded" in str(exc.value)

    @pytest.mark.asyncio
    async def test_mixed_sync_async_permissions(self):
        """Test mixing sync and async permission checks."""

        # Sync check
        def is_authenticated(info, *args, **kwargs) -> bool:
            return info.context.get("user") is not None

        # Async check
        async def has_premium_subscription(info, *args, **kwargs) -> bool:
            await asyncio.sleep(0.01)  # Simulate async check
            user = info.context.get("user", {})
            return user.get("subscription") == "premium"

        # Combined check
        combined = combine_permissions(is_authenticated, has_premium_subscription)

        @fraiseql.type
        class PremiumContent:
            title: str

            @field
            @authorize_field(combined)
            async def premium_data(self, info) -> str:
                return "premium content"

        content = PremiumContent(title="Premium Article")
        info = MagicMock(spec=GraphQLResolveInfo)
        resolver = PremiumContent.premium_data

        # Not authenticated
        info.context = {}
        with pytest.raises(FieldAuthorizationError):
            await resolver(content, info)

        # Authenticated but no premium
        info.context = {"user": {"id": 1, "subscription": "basic"}}
        with pytest.raises(FieldAuthorizationError):
            await resolver(content, info)

        # Authenticated with premium
        info.context = {"user": {"id": 1, "subscription": "premium"}}
        result = await resolver(content, info)
        assert result == "premium content"

    def test_context_based_field_visibility(self):
        """Test fields that are conditionally visible based on context."""

        def can_see_field(field_name: str):
            """Factory for field-specific permission checks."""

            def check(info, *args, **kwargs) -> bool:
                user = info.context.get("user", {})
                visible_fields = user.get("visible_fields", [])
                return field_name in visible_fields

            return check

        @fraiseql.type
        class FlexibleObject:
            id: int

            @field
            @authorize_field(can_see_field("email"))
            def email(self, info) -> str:
                return "user@example.com"

            @field
            @authorize_field(can_see_field("phone"))
            def phone(self, info) -> str:
                return "+1234567890"

            @field
            @authorize_field(can_see_field("address"))
            def address(self, info) -> str:
                return "123 Main St"

        obj = FlexibleObject(id=1)
        info = MagicMock(spec=GraphQLResolveInfo)

        # User with access to email only
        info.context = {"user": {"visible_fields": ["email"]}}
        assert FlexibleObject.email(obj, info) == "user@example.com"

        with pytest.raises(FieldAuthorizationError):
            FlexibleObject.phone(obj, info)

        with pytest.raises(FieldAuthorizationError):
            FlexibleObject.address(obj, info)

        # User with access to all fields
        info.context = {"user": {"visible_fields": ["email", "phone", "address"]}}
        assert FlexibleObject.email(obj, info) == "user@example.com"
        assert FlexibleObject.phone(obj, info) == "+1234567890"
        assert FlexibleObject.address(obj, info) == "123 Main St"

    def test_permission_with_custom_error_codes(self):
        """Test permissions that return specific error codes."""

        class CustomAuthError(FieldAuthorizationError):
            def __init__(self, message: str, code: str):
                super().__init__(message)
                self.extensions = {"code": code, "type": "AUTHORIZATION_ERROR"}

        def check_subscription_tier(required_tier: str):
            def check(info, *args, **kwargs) -> bool:
                user = info.context.get("user", {})
                user_tier = user.get("tier", "free")

                tiers = ["free", "basic", "premium", "enterprise"]
                required_level = tiers.index(required_tier)
                user_level = tiers.index(user_tier)

                # This is a limitation - we can't raise custom errors from permission check
                # But we can use the error_message parameter
                return user_level >= required_level

            return check

        @fraiseql.type
        class TieredService:
            @field
            @authorize_field(
                check_subscription_tier("premium"), error_message="Premium subscription required"
            )
            def premium_feature(self, info) -> str:
                return "premium"

            @field
            @authorize_field(
                check_subscription_tier("enterprise"),
                error_message="Enterprise subscription required",
            )
            def enterprise_feature(self, info) -> str:
                return "enterprise"

        service = TieredService()
        info = MagicMock(spec=GraphQLResolveInfo)

        # Free user
        info.context = {"user": {"tier": "free"}}

        with pytest.raises(FieldAuthorizationError) as exc:
            TieredService.premium_feature(service, info)
        assert "Premium subscription required" in str(exc.value)

        # Premium user can access premium but not enterprise
        info.context = {"user": {"tier": "premium"}}
        assert TieredService.premium_feature(service, info) == "premium"

        with pytest.raises(FieldAuthorizationError) as exc:
            TieredService.enterprise_feature(service, info)
        assert "Enterprise subscription required" in str(exc.value)
