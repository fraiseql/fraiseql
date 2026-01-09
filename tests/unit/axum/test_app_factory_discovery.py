"""Unit tests for app factory with discovery support (Phase D.4)."""

import pytest

from fraiseql.axum.app import create_axum_fraiseql_app
from fraiseql.axum.registry import AxumRegistry


@pytest.fixture(autouse=True)
def clear_registry():
    """Clear registry before and after each test."""
    # Clear singleton registry
    singleton = AxumRegistry.get_instance()
    singleton.clear()
    yield
    # Clear after test
    AxumRegistry.get_instance().clear()


class TestAppFactoryBasics:
    """Tests for basic app factory functionality."""

    def test_create_app_without_discovery(self) -> None:
        """Test creating app without auto-discovery (backward compatibility)."""
        app = create_axum_fraiseql_app(
            database_url="postgresql://user:pass@localhost/db",
            auto_discover=False,
        )

        assert app is not None
        assert len(app.registered_types()) == 0
        assert len(app.registered_queries()) == 0

    def test_create_app_with_explicit_types(self) -> None:
        """Test creating app with explicit types (backward compatibility)."""

        class User:
            pass

        app = create_axum_fraiseql_app(
            database_url="postgresql://user:pass@localhost/db",
            types=[User],
        )

        assert len(app.registered_types()) == 1
        assert "User" in app.registered_types()

    def test_create_app_registers_to_registry(self) -> None:
        """Test that explicit registration also registers to registry."""

        class User:
            pass

        app = create_axum_fraiseql_app(
            database_url="postgresql://user:pass@localhost/db",
            types=[User],
        )

        registry = app.get_registry()
        types = registry.get_registered_types()
        assert "User" in types
        assert types["User"] is User

    def test_create_app_with_custom_registry(self) -> None:
        """Test creating app with custom registry instance."""
        custom_registry = AxumRegistry()
        custom_registry.clear()

        class User:
            pass

        app = create_axum_fraiseql_app(
            database_url="postgresql://user:pass@localhost/db",
            types=[User],
            registry=custom_registry,
        )

        assert app.get_registry() is custom_registry
        assert "User" in custom_registry.get_registered_types()




class TestBackwardCompatibility:
    """Tests for backward compatibility with explicit lists."""

    def test_explicit_types_still_work(self) -> None:
        """Test that explicit types parameter still works."""

        class User:
            pass

        class Post:
            pass

        app = create_axum_fraiseql_app(
            database_url="postgresql://user:pass@localhost/db",
            types=[User, Post],
        )

        assert len(app.registered_types()) == 2
        assert "User" in app.registered_types()
        assert "Post" in app.registered_types()

    def test_explicit_queries_still_work(self) -> None:
        """Test that explicit queries parameter still works."""

        async def get_users():
            pass

        async def get_posts():
            pass

        app = create_axum_fraiseql_app(
            database_url="postgresql://user:pass@localhost/db",
            queries=[get_users, get_posts],
        )

        assert len(app.registered_queries()) == 2
        assert "get_users" in app.registered_queries()
        assert "get_posts" in app.registered_queries()

    def test_explicit_mutations_still_work(self) -> None:
        """Test that explicit mutations parameter still works."""

        async def create_user():
            pass

        async def delete_user():
            pass

        app = create_axum_fraiseql_app(
            database_url="postgresql://user:pass@localhost/db",
            mutations=[create_user, delete_user],
        )

        assert len(app.registered_mutations()) == 2
        assert "create_user" in app.registered_mutations()
        assert "delete_user" in app.registered_mutations()

    def test_explicit_subscriptions_still_work(self) -> None:
        """Test that explicit subscriptions parameter still works."""

        async def on_user_created():
            pass

        async def on_user_updated():
            pass

        app = create_axum_fraiseql_app(
            database_url="postgresql://user:pass@localhost/db",
            subscriptions=[on_user_created, on_user_updated],
        )

        assert len(app.registered_subscriptions()) == 2
        assert "on_user_created" in app.registered_subscriptions()
        assert "on_user_updated" in app.registered_subscriptions()

    def test_multiple_explicit_categories(self) -> None:
        """Test combining multiple explicit categories."""

        class User:
            pass

        async def get_users():
            pass

        async def create_user():
            pass

        async def on_user_created():
            pass

        app = create_axum_fraiseql_app(
            database_url="postgresql://user:pass@localhost/db",
            types=[User],
            queries=[get_users],
            mutations=[create_user],
            subscriptions=[on_user_created],
        )

        assert len(app.registered_types()) == 1
        assert len(app.registered_queries()) == 1
        assert len(app.registered_mutations()) == 1
        assert len(app.registered_subscriptions()) == 1






class TestAppFactoryErrors:
    """Tests for error handling in app factory."""

    def test_missing_database_url_raises_error(self) -> None:
        """Test that missing database_url raises ValueError."""
        with pytest.raises(ValueError, match="database_url is required"):
            create_axum_fraiseql_app()

    def test_database_url_from_kwargs(self) -> None:
        """Test that database_url can be passed via kwargs."""
        app = create_axum_fraiseql_app(
            database_url="postgresql://user:pass@localhost/db",
        )

        assert app.get_config().database_url == "postgresql://user:pass@localhost/db"



class TestRegistryIntegrationWithAppFactory:
    """Tests for registry integration with app factory.

    The discovery system (Phase D.2) finds items; the registry (Phase D.1)
    stores them; the app factory (Phase D.4) registers them explicitly.
    These tests verify the integration between these systems.
    """

    def test_fraiseql_decorated_types_via_registry(self) -> None:
        """Test that decorated types flow through registry to app."""
        # Create a module-like namespace with GraphQL items
        # In real usage, these would be in separate modules
        class DiscoveredUser:
            pass

        class DiscoveredPost:
            pass

        # Mark them as FraiseQL types (as decorators would do)
        DiscoveredUser._fraiseql_type = True
        DiscoveredPost._fraiseql_type = True

        # Manually register to registry to simulate discovery
        registry = AxumRegistry.get_instance()
        registry.clear()
        registry.register_type(DiscoveredUser)
        registry.register_type(DiscoveredPost)

        # Now create app with explicit types (simulating discovered items)
        app = create_axum_fraiseql_app(
            database_url="postgresql://user:pass@localhost/db",
            types=[DiscoveredUser, DiscoveredPost],
        )

        # Verify both are registered
        assert len(app.registered_types()) == 2
        assert "DiscoveredUser" in app.registered_types()
        assert "DiscoveredPost" in app.registered_types()

        # Verify they're in registry
        registry_types = app.get_registry().get_registered_types()
        assert "DiscoveredUser" in registry_types
        assert "DiscoveredPost" in registry_types

    def test_fraiseql_decorated_queries_via_registry(self) -> None:
        """Test that decorated queries flow through registry to app."""
        async def discovered_get_users():
            pass

        async def discovered_get_posts():
            pass

        # Mark as queries (as decorators would do)
        discovered_get_users._fraiseql_query = True
        discovered_get_posts._fraiseql_query = True

        # Register to registry
        registry = AxumRegistry.get_instance()
        registry.clear()
        registry.register_query(discovered_get_users)
        registry.register_query(discovered_get_posts)

        # Create app (simulating discovered queries)
        app = create_axum_fraiseql_app(
            database_url="postgresql://user:pass@localhost/db",
            queries=[discovered_get_users, discovered_get_posts],
        )

        assert len(app.registered_queries()) == 2
        assert "discovered_get_users" in app.registered_queries()
        assert "discovered_get_posts" in app.registered_queries()

    def test_full_schema_via_explicit_registration(self) -> None:
        """Test complete GraphQL schema with explicit registration."""
        # Types
        class User:
            pass

        class Post:
            pass

        # Input
        class CreateUserInput:
            pass

        # Enum
        class UserRole:
            ADMIN = "admin"
            USER = "user"

        # Interface
        class Node:
            pass

        # Queries
        async def get_users():
            pass

        # Mutations
        async def create_user():
            pass

        # Subscriptions
        async def on_user_created():
            pass

        # Pre-register everything (simulating discovery)
        registry = AxumRegistry.get_instance()
        registry.clear()

        registry.register_type(User)
        registry.register_type(Post)
        registry.register_input(CreateUserInput)
        registry.register_enum(UserRole)
        registry.register_interface(Node)
        registry.register_query(get_users)
        registry.register_mutation(create_user)
        registry.register_subscription(on_user_created)

        # Create app with discovered items (via explicit lists)
        app = create_axum_fraiseql_app(
            database_url="postgresql://user:pass@localhost/db",
            types=[User, Post],
            queries=[get_users],
            mutations=[create_user],
            subscriptions=[on_user_created],
        )

        # Verify everything is registered
        assert len(app.registered_types()) == 2
        assert len(app.registered_queries()) == 1
        assert len(app.registered_mutations()) == 1
        assert len(app.registered_subscriptions()) == 1

        # Verify registry has all items
        counts = app.get_registry().count_registered()
        assert counts["types"] >= 2
        assert counts["queries"] >= 1
        assert counts["mutations"] >= 1
        assert counts["subscriptions"] >= 1

    def test_discovery_result_items_with_app_factory(self) -> None:
        """Test that items found by discovery can be explicitly registered."""
        from fraiseql.axum.discovery import DiscoveryResult

        # Create discovery result with items
        result = DiscoveryResult(source="test.module")

        class TestType:
            pass

        async def test_query():
            pass

        result.types_found.append(TestType)
        result.queries_found.append(test_query)

        # Create app using discovered items
        app = create_axum_fraiseql_app(
            database_url="postgresql://user:pass@localhost/db",
            types=result.types_found,
            queries=result.queries_found,
        )

        # Verify discovery results are in app
        assert "TestType" in app.registered_types()
        assert "test_query" in app.registered_queries()
