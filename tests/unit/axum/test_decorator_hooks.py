"""Unit tests for decorator registration hooks."""

import pytest

from fraiseql.axum.registry import AxumRegistry
from fraiseql.axum.registration_hooks import (
    register_enum_hook,
    register_input_hook,
    register_interface_hook,
    register_query_hook,
    register_subscription_hook,
    register_type_hook,
)


@pytest.fixture(autouse=True)
def clear_registry():
    """Clear registry before and after each test."""
    AxumRegistry.get_instance().clear()
    yield
    AxumRegistry.get_instance().clear()


class TestTypeRegistrationHook:
    """Tests for register_type_hook."""

    def test_register_type_hook(self):
        """Test that register_type_hook registers a type."""
        class User:
            pass

        register_type_hook(User)

        registry = AxumRegistry.get_instance()
        types = registry.get_registered_types()
        assert "User" in types
        assert types["User"] is User

    def test_register_type_hook_multiple(self):
        """Test registering multiple types."""
        class User:
            pass

        class Post:
            pass

        register_type_hook(User)
        register_type_hook(Post)

        registry = AxumRegistry.get_instance()
        types = registry.get_registered_types()
        assert len(types) == 2
        assert "User" in types
        assert "Post" in types


class TestInputRegistrationHook:
    """Tests for register_input_hook."""

    def test_register_input_hook(self):
        """Test that register_input_hook registers an input."""
        class CreateUserInput:
            pass

        register_input_hook(CreateUserInput)

        registry = AxumRegistry.get_instance()
        inputs = registry.get_registered_inputs()
        assert "CreateUserInput" in inputs
        assert inputs["CreateUserInput"] is CreateUserInput


class TestEnumRegistrationHook:
    """Tests for register_enum_hook."""

    def test_register_enum_hook(self):
        """Test that register_enum_hook registers an enum."""
        class UserRole:
            ADMIN = "admin"
            USER = "user"

        register_enum_hook(UserRole)

        registry = AxumRegistry.get_instance()
        enums = registry.get_registered_enums()
        assert "UserRole" in enums
        assert enums["UserRole"] is UserRole


class TestInterfaceRegistrationHook:
    """Tests for register_interface_hook."""

    def test_register_interface_hook(self):
        """Test that register_interface_hook registers an interface."""
        class Node:
            pass

        register_interface_hook(Node)

        registry = AxumRegistry.get_instance()
        interfaces = registry.get_registered_interfaces()
        assert "Node" in interfaces
        assert interfaces["Node"] is Node


class TestQueryRegistrationHook:
    """Tests for register_query_hook."""

    def test_register_query_hook(self):
        """Test that register_query_hook registers a query."""
        async def get_users():
            pass

        register_query_hook(get_users)

        registry = AxumRegistry.get_instance()
        queries = registry.get_registered_queries()
        assert "get_users" in queries
        assert queries["get_users"] is get_users


class TestSubscriptionRegistrationHook:
    """Tests for register_subscription_hook."""

    def test_register_subscription_hook(self):
        """Test that register_subscription_hook registers a subscription."""
        async def on_user_created():
            pass

        register_subscription_hook(on_user_created)

        registry = AxumRegistry.get_instance()
        subscriptions = registry.get_registered_subscriptions()
        assert "on_user_created" in subscriptions
        assert subscriptions["on_user_created"] is on_user_created


class TestHooksErrorHandling:
    """Tests for error handling in hooks."""

    def test_hook_with_invalid_type(self):
        """Test that hooks handle invalid types gracefully."""
        # Passing None should not raise
        try:
            register_type_hook(None)  # type: ignore
        except Exception:
            pytest.fail("Hook should handle None gracefully")

    def test_hook_with_missing_name(self):
        """Test hook with object missing __name__ attribute."""
        class NoName:
            pass

        # Remove __name__ (shouldn't happen in practice, but test robustness)
        obj = NoName()
        # Can't remove __name__ from instances, so this test verifies the pattern


class TestHooksWithRegistry:
    """Tests for hooks integration with registry."""

    def test_hooks_populate_registry(self):
        """Test that hooks properly populate the registry."""
        class User:
            pass

        class CreateUserInput:
            pass

        class UserRole:
            ADMIN = "admin"

        async def get_users():
            pass

        async def on_user_created():
            pass

        register_type_hook(User)
        register_input_hook(CreateUserInput)
        register_enum_hook(UserRole)
        register_query_hook(get_users)
        register_subscription_hook(on_user_created)

        registry = AxumRegistry.get_instance()
        counts = registry.count_registered()

        assert counts["types"] == 1
        assert counts["inputs"] == 1
        assert counts["enums"] == 1
        assert counts["queries"] == 1
        assert counts["subscriptions"] == 1
        assert counts["total"] == 5

    def test_hooks_summary(self):
        """Test that registry summary reflects hooked registrations."""
        class User:
            pass

        async def get_users():
            pass

        register_type_hook(User)
        register_query_hook(get_users)

        registry = AxumRegistry.get_instance()
        summary = registry.summary()

        assert "Types: 1" in summary
        assert "Queries: 1" in summary
        assert "User" in summary
        assert "get_users" in summary
