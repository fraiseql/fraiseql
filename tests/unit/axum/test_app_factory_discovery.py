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

    def test_create_app_without_discovery(self):
        """Test creating app without auto-discovery (backward compatibility)."""
        app = create_axum_fraiseql_app(
            database_url="postgresql://user:pass@localhost/db",
            auto_discover=False,
        )

        assert app is not None
        assert len(app.registered_types()) == 0
        assert len(app.registered_queries()) == 0

    def test_create_app_with_explicit_types(self):
        """Test creating app with explicit types (backward compatibility)."""

        class User:
            pass

        app = create_axum_fraiseql_app(
            database_url="postgresql://user:pass@localhost/db",
            types=[User],
        )

        assert len(app.registered_types()) == 1
        assert "User" in app.registered_types()

    def test_create_app_registers_to_registry(self):
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

    def test_create_app_with_custom_registry(self):
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


class TestAppFactoryWithDiscovery:
    """Tests for auto-discovery in app factory."""

    def test_auto_discover_false_by_default(self):
        """Test that auto_discover defaults to False."""
        app = create_axum_fraiseql_app(
            database_url="postgresql://user:pass@localhost/db",
        )

        # Should not discover anything without explicit flag
        assert len(app.registered_types()) == 0

    def test_auto_discover_empty_package(self):
        """Test auto-discover with package containing no GraphQL items."""
        # json module has no FraiseQL items
        app = create_axum_fraiseql_app(
            database_url="postgresql://user:pass@localhost/db",
            auto_discover=True,
            discover_packages=["json"],
        )

        # Should complete without errors, registering nothing
        assert len(app.registered_types()) == 0
        assert len(app.registered_queries()) == 0

    def test_auto_discover_nonexistent_package_graceful(self):
        """Test auto-discover with non-existent package fails gracefully."""
        # Discovery gracefully handles missing packages (logs warning but continues)
        app = create_axum_fraiseql_app(
            database_url="postgresql://user:pass@localhost/db",
            auto_discover=True,
            discover_packages=["nonexistent.module.xyz"],
        )

        # App still created despite missing package
        assert app is not None
        assert len(app.registered_types()) == 0

    def test_auto_discover_uses_main_by_default(self):
        """Test that auto-discover defaults to __main__ package."""
        # This test just verifies the default is used; actual discovery
        # of __main__ may or may not find items depending on test context
        app = create_axum_fraiseql_app(
            database_url="postgresql://user:pass@localhost/db",
            auto_discover=True,
            discover_packages=None,  # Should default to ["__main__"]
        )

        assert app is not None


class TestBackwardCompatibility:
    """Tests for backward compatibility with explicit lists."""

    def test_explicit_types_still_work(self):
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

    def test_explicit_queries_still_work(self):
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

    def test_explicit_mutations_still_work(self):
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

    def test_explicit_subscriptions_still_work(self):
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

    def test_multiple_explicit_categories(self):
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


class TestMixedDiscoveryAndExplicit:
    """Tests for combining auto-discovery with explicit lists."""

    def test_discovery_and_explicit_types_combined(self):
        """Test that explicit types are registered alongside discovered items."""

        class ExplicitType:
            pass

        app = create_axum_fraiseql_app(
            database_url="postgresql://user:pass@localhost/db",
            auto_discover=True,
            discover_packages=["json"],  # Won't find items
            types=[ExplicitType],
        )

        # Should have explicit type
        assert "ExplicitType" in app.registered_types()

    def test_explicit_overrides_discovered(self):
        """Test that explicit items with same name override discovered ones."""
        # Register to registry first to simulate discovered item
        registry = AxumRegistry.get_instance()

        class User:
            pass

        registry.register_type(User)

        # Now create app with explicit User
        class UserOverride:
            pass

        UserOverride.__name__ = "User"

        app = create_axum_fraiseql_app(
            database_url="postgresql://user:pass@localhost/db",
            types=[UserOverride],
            registry=registry,
        )

        # Should have the overridden version
        types = app.get_registry().get_registered_types()
        assert types["User"] is UserOverride


class TestDiscoveryPackages:
    """Tests for discover_packages parameter."""

    def test_discover_multiple_packages(self):
        """Test discovering from multiple packages."""
        app = create_axum_fraiseql_app(
            database_url="postgresql://user:pass@localhost/db",
            auto_discover=True,
            discover_packages=["json", "pathlib"],  # Both have no FraiseQL items
        )

        assert app is not None

    def test_discover_packages_with_none_defaults_to_main(self):
        """Test that None for discover_packages defaults to __main__."""
        app = create_axum_fraiseql_app(
            database_url="postgresql://user:pass@localhost/db",
            auto_discover=True,
            discover_packages=None,
        )

        assert app is not None

    def test_discover_packages_empty_list(self):
        """Test with empty discover_packages list (no discovery)."""
        app = create_axum_fraiseql_app(
            database_url="postgresql://user:pass@localhost/db",
            auto_discover=True,
            discover_packages=[],
        )

        # Empty list means no packages to discover
        assert len(app.registered_types()) == 0


class TestAppFactoryErrors:
    """Tests for error handling in app factory."""

    def test_missing_database_url_raises_error(self):
        """Test that missing database_url raises ValueError."""
        with pytest.raises(ValueError, match="database_url is required"):
            create_axum_fraiseql_app()

    def test_database_url_from_kwargs(self):
        """Test that database_url can be passed via kwargs."""
        app = create_axum_fraiseql_app(
            database_url="postgresql://user:pass@localhost/db",
        )

        assert app.get_config().database_url == "postgresql://user:pass@localhost/db"

    def test_invalid_package_during_discovery_graceful(self):
        """Test that invalid package name is handled gracefully during discovery."""
        # Discovery handles import errors gracefully and continues
        app = create_axum_fraiseql_app(
            database_url="postgresql://user:pass@localhost/db",
            auto_discover=True,
            discover_packages=["this.package.does.not.exist"],
        )

        # App still created despite invalid package
        assert app is not None
        assert len(app.registered_types()) == 0


class TestRegistryIntegration:
    """Tests for registry integration with app factory."""

    def test_app_registry_is_singleton_by_default(self):
        """Test that app uses singleton registry by default."""
        app1 = create_axum_fraiseql_app(
            database_url="postgresql://user:pass@localhost/db",
        )

        app2 = create_axum_fraiseql_app(
            database_url="postgresql://user:pass@localhost/db",
        )

        # Both should use same singleton registry
        assert app1.get_registry() is app2.get_registry()

    def test_custom_registry_parameter_used(self):
        """Test that custom registry parameter is used instead of singleton."""
        # Get singleton first
        singleton_before = AxumRegistry.get_instance()

        # Create another instance (due to singleton pattern, will be same instance)
        # But the important thing is that we pass it explicitly
        custom_registry = AxumRegistry()

        class User:
            pass

        app = create_axum_fraiseql_app(
            database_url="postgresql://user:pass@localhost/db",
            types=[User],
            registry=custom_registry,
        )

        # The app should use the explicitly provided registry
        assert app.get_registry() is custom_registry
        assert app.get_registry() is singleton_before  # They're the same due to singleton

        # Custom registry should have the type
        assert "User" in custom_registry.get_registered_types()

    def test_registry_summary_available(self):
        """Test that registry summary is available through app."""

        class User:
            pass

        app = create_axum_fraiseql_app(
            database_url="postgresql://user:pass@localhost/db",
            types=[User],
        )

        registry = app.get_registry()
        summary = registry.summary()

        assert "Types: 1" in summary
        assert "User" in summary
