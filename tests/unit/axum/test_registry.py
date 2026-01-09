"""Unit tests for AxumRegistry."""

import pytest

from fraiseql.axum.registry import AxumRegistry


@pytest.fixture(autouse=True)
def clear_registry():
    """Clear registry before and after each test."""
    AxumRegistry.get_instance().clear()
    yield
    AxumRegistry.get_instance().clear()


class TestAxumRegistry:
    """Tests for AxumRegistry singleton and registration."""

    def test_singleton_pattern(self) -> None:
        """Test that AxumRegistry is a singleton."""
        registry1 = AxumRegistry.get_instance()
        registry2 = AxumRegistry.get_instance()

        assert registry1 is registry2, "Registry should be a singleton"

    def test_register_type(self) -> None:
        """Test registering a GraphQL type."""
        registry = AxumRegistry.get_instance()

        class User:
            pass

        registry.register_type(User)

        types = registry.get_registered_types()
        assert "User" in types
        assert types["User"] is User

    def test_register_types_batch(self) -> None:
        """Test batch registration of types."""
        registry = AxumRegistry.get_instance()

        class User:
            pass

        class Post:
            pass

        class Comment:
            pass

        registry.register_types([User, Post, Comment])

        types = registry.get_registered_types()
        assert len(types) == 3
        assert all(name in types for name in ["User", "Post", "Comment"])

    def test_register_query(self) -> None:
        """Test registering a query."""
        registry = AxumRegistry.get_instance()

        async def get_users():
            pass

        registry.register_query(get_users)

        queries = registry.get_registered_queries()
        assert "get_users" in queries
        assert queries["get_users"] is get_users

    def test_register_mutation(self) -> None:
        """Test registering a mutation."""
        registry = AxumRegistry.get_instance()

        async def create_user(name: str):
            pass

        registry.register_mutation(create_user)

        mutations = registry.get_registered_mutations()
        assert "create_user" in mutations
        assert mutations["create_user"] is create_user

    def test_register_subscription(self) -> None:
        """Test registering a subscription."""
        registry = AxumRegistry.get_instance()

        async def on_user_created():
            pass

        registry.register_subscription(on_user_created)

        subscriptions = registry.get_registered_subscriptions()
        assert "on_user_created" in subscriptions
        assert subscriptions["on_user_created"] is on_user_created

    def test_register_input(self) -> None:
        """Test registering an input type."""
        registry = AxumRegistry.get_instance()

        class CreateUserInput:
            pass

        registry.register_input(CreateUserInput)

        inputs = registry.get_registered_inputs()
        assert "CreateUserInput" in inputs
        assert inputs["CreateUserInput"] is CreateUserInput

    def test_register_enum(self) -> None:
        """Test registering an enum type."""
        registry = AxumRegistry.get_instance()

        class UserRole:
            ADMIN = "admin"
            USER = "user"

        registry.register_enum(UserRole)

        enums = registry.get_registered_enums()
        assert "UserRole" in enums
        assert enums["UserRole"] is UserRole

    def test_register_interface(self) -> None:
        """Test registering an interface."""
        registry = AxumRegistry.get_instance()

        class Node:
            pass

        registry.register_interface(Node)

        interfaces = registry.get_registered_interfaces()
        assert "Node" in interfaces
        assert interfaces["Node"] is Node

    def test_count_registered(self) -> None:
        """Test count_registered method."""
        registry = AxumRegistry.get_instance()

        class User:
            pass

        class CreateUserInput:
            pass

        async def get_users():
            pass

        async def create_user(input: CreateUserInput):
            pass

        registry.register_type(User)
        registry.register_input(CreateUserInput)
        registry.register_query(get_users)
        registry.register_mutation(create_user)

        counts = registry.count_registered()
        assert counts["types"] == 1
        assert counts["inputs"] == 1
        assert counts["queries"] == 1
        assert counts["mutations"] == 1
        assert counts["total"] == 4

    def test_summary(self) -> None:
        """Test summary method."""
        registry = AxumRegistry.get_instance()

        class User:
            pass

        async def get_users():
            pass

        registry.register_type(User)
        registry.register_query(get_users)

        summary = registry.summary()
        assert "AxumRegistry Summary:" in summary
        assert "User" in summary
        assert "get_users" in summary

    def test_clear(self) -> None:
        """Test clearing the registry."""
        registry = AxumRegistry.get_instance()

        class User:
            pass

        async def get_users():
            pass

        registry.register_type(User)
        registry.register_query(get_users)

        assert len(registry.get_registered_types()) == 1
        assert len(registry.get_registered_queries()) == 1

        registry.clear()

        assert len(registry.get_registered_types()) == 0
        assert len(registry.get_registered_queries()) == 0

    def test_to_lists(self) -> None:
        """Test to_lists conversion method."""
        registry = AxumRegistry.get_instance()

        class User:
            pass

        class CreateUser:
            pass

        async def get_users():
            pass

        async def create_user(input: CreateUser):
            pass

        registry.register_type(User)
        registry.register_mutation(CreateUser)
        registry.register_query(get_users)
        registry.register_mutation(create_user)

        types, mutations, queries, subscriptions = registry.to_lists()

        assert User in types
        assert len(mutations) == 2
        assert get_users in queries
        assert len(subscriptions) == 0

    def test_empty_registry_summary(self) -> None:
        """Test summary of empty registry."""
        registry = AxumRegistry.get_instance()

        summary = registry.summary()
        assert "(empty)" in summary
        assert "AxumRegistry Summary:" in summary

    def test_register_with_custom_name(self) -> None:
        """Test that items are registered by __name__ attribute."""
        registry = AxumRegistry.get_instance()

        class User:
            pass

        # Manually set __name__ if needed (for edge cases)
        registry.register_type(User)

        types = registry.get_registered_types()
        assert "User" in types


class TestAxumRegistryIsolation:
    """Tests for registry isolation between tests."""

    def test_isolation_between_tests(self) -> None:
        """Test that clear() provides test isolation."""
        registry = AxumRegistry.get_instance()

        class TestType1:
            pass

        registry.register_type(TestType1)
        assert len(registry.get_registered_types()) == 1

        # clear_registry fixture clears before next test
        # This test checks that the fixture works

    def test_isolation_second_test(self) -> None:
        """Second test to verify isolation from previous test."""
        registry = AxumRegistry.get_instance()
        # Should be empty due to clear_registry fixture
        assert len(registry.get_registered_types()) == 0


class TestAxumRegistryEdgeCases:
    """Tests for edge cases and error conditions."""

    def test_register_duplicate_types(self) -> None:
        """Test registering the same type twice (should overwrite)."""
        registry = AxumRegistry.get_instance()

        class User:
            pass

        registry.register_type(User)
        registry.register_type(User)

        types = registry.get_registered_types()
        assert len(types) == 1  # Should still be 1, not 2

    def test_batch_register_empty_list(self) -> None:
        """Test batch registering empty lists."""
        registry = AxumRegistry.get_instance()

        registry.register_types([])
        registry.register_queries([])
        registry.register_mutations([])
        registry.register_subscriptions([])

        counts = registry.count_registered()
        assert counts["total"] == 0

    def test_get_returns_copy(self) -> None:
        """Test that getter methods return copies, not references."""
        registry = AxumRegistry.get_instance()

        class User:
            pass

        registry.register_type(User)

        types1 = registry.get_registered_types()
        types1.pop("User")

        types2 = registry.get_registered_types()
        assert "User" in types2  # Should still be there
