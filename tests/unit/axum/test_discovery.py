"""Unit tests for auto-discovery system."""

import pytest

from fraiseql.axum.discovery import (
    DiscoveryResult,
    discover_from_module,
    discover_from_package,
)
from fraiseql.axum.registry import AxumRegistry


@pytest.fixture(autouse=True)
def clear_registry():
    """Clear registry before and after each test."""
    AxumRegistry.get_instance().clear()
    yield
    AxumRegistry.get_instance().clear()


class TestDiscoveryResult:
    """Tests for DiscoveryResult dataclass."""

    def test_discovery_result_init(self) -> None:
        """Test DiscoveryResult initialization."""
        result = DiscoveryResult(source="test.module")

        assert result.source == "test.module"
        assert result.types_found == []
        assert result.mutations_found == []
        assert result.queries_found == []
        assert result.errors == []

    def test_count_total(self) -> None:
        """Test count_total method."""
        result = DiscoveryResult(source="test")

        class User:
            pass

        async def get_users():
            pass

        result.types_found.append(User)
        result.queries_found.append(get_users)

        assert result.count_total() == 2

    def test_summary_empty(self) -> None:
        """Test summary with no items."""
        result = DiscoveryResult(source="test.empty")

        summary = result.summary()
        assert "Discovery Result for test.empty:" in summary
        assert "(no items found)" in summary

    def test_summary_with_items(self) -> None:
        """Test summary with items."""
        result = DiscoveryResult(source="test.types")

        class User:
            pass

        class Post:
            pass

        result.types_found.extend([User, Post])

        summary = result.summary()
        assert "Types: 2" in summary
        assert "User" in summary
        assert "Post" in summary

    def test_summary_with_errors(self) -> None:
        """Test summary with errors."""
        result = DiscoveryResult(source="test.error")

        error = ImportError("Module not found")
        result.errors.append(error)

        summary = result.summary()
        assert "Errors: 1" in summary
        assert "ImportError" in summary

    def test_register_to_registry(self) -> None:
        """Test registering items to registry."""
        result = DiscoveryResult(source="test.registry")

        class User:
            pass

        async def get_users():
            pass

        result.types_found.append(User)
        result.queries_found.append(get_users)

        result.register_to_registry()

        registry = AxumRegistry.get_instance()
        assert len(registry.get_registered_types()) == 1
        assert len(registry.get_registered_queries()) == 1


class TestDiscoverFromModule:
    """Tests for discover_from_module function."""

    def test_discover_nonexistent_module(self) -> None:
        """Test discovering from non-existent module."""
        result = discover_from_module("nonexistent.module.that.does.not.exist")

        assert len(result.errors) == 1
        assert isinstance(result.errors[0], ImportError)
        assert result.count_total() == 0

    def test_discover_empty_module(self) -> None:
        """Test discovering from module with no GraphQL items."""
        # Use a real empty module
        result = discover_from_module("json")  # Standard library with no FraiseQL items

        # json module won't have _fraiseql markers, so discovery should be empty
        assert result.count_total() == 0
        assert len(result.errors) == 0

    def test_discover_result_has_source(self) -> None:
        """Test that DiscoveryResult has correct source."""
        result = discover_from_module("json")

        assert result.source == "json"


class TestDiscoverFromPackage:
    """Tests for discover_from_package function."""

    def test_discover_nonexistent_package(self) -> None:
        """Test discovering from non-existent package."""
        result = discover_from_package("nonexistent.package.xyz")

        assert len(result.errors) > 0
        assert result.count_total() == 0

    def test_discover_package_result_has_source(self) -> None:
        """Test that DiscoveryResult has correct source for package."""
        result = discover_from_package("fraiseql.axum")

        # axum package should exist and have some items
        assert result.source == "fraiseql.axum"


class TestDiscoveryIntegration:
    """Integration tests for discovery with registry."""

    def test_discovery_auto_registers(self) -> None:
        """Test that discovery result can register to registry."""
        result = DiscoveryResult(source="test.types")

        class User:
            pass

        class Post:
            pass

        async def get_users():
            pass

        result.types_found.extend([User, Post])
        result.queries_found.append(get_users)

        # Before registration
        registry = AxumRegistry.get_instance()
        assert registry.count_registered()["total"] == 0

        # Register
        result.register_to_registry()

        # After registration
        assert registry.count_registered()["types"] == 2
        assert registry.count_registered()["queries"] == 1

    def test_discovery_with_all_item_types(self) -> None:
        """Test discovery and registration of all item types."""
        result = DiscoveryResult(source="test.all")

        class User:
            pass

        class CreateUserInput:
            pass

        class UserRole:
            pass

        class Node:
            pass

        async def get_users():
            pass

        async def create_user(input_: CreateUserInput):
            pass

        async def on_user_created():
            pass

        result.types_found.append(User)
        result.inputs_found.append(CreateUserInput)
        result.enums_found.append(UserRole)
        result.interfaces_found.append(Node)
        result.queries_found.append(get_users)
        result.mutations_found.append(create_user)
        result.subscriptions_found.append(on_user_created)

        result.register_to_registry()

        registry = AxumRegistry.get_instance()
        counts = registry.count_registered()

        assert counts["types"] == 1
        assert counts["inputs"] == 1
        assert counts["enums"] == 1
        assert counts["interfaces"] == 1
        assert counts["queries"] == 1
        assert counts["mutations"] == 1
        assert counts["subscriptions"] == 1
        assert counts["total"] == 7


class TestDiscoveryEdgeCases:
    """Tests for edge cases in discovery."""

    def test_discovery_empty_source(self) -> None:
        """Test DiscoveryResult with empty source."""
        result = DiscoveryResult(source="")

        assert result.source == ""
        assert result.count_total() == 0

    def test_discovery_result_with_duplicates(self) -> None:
        """Test DiscoveryResult with duplicate items."""
        result = DiscoveryResult(source="test.dupes")

        class User:
            pass

        # Add same type twice
        result.types_found.append(User)
        result.types_found.append(User)

        # Should count both (duplicates not deduplicated at discovery level)
        assert result.count_total() == 2

    def test_discovery_summary_long_lists(self) -> None:
        """Test summary formatting with many items."""
        result = DiscoveryResult(source="test.many")

        # Add many items
        for i in range(5):

            class Type:
                pass

            Type.__name__ = f"Type{i}"
            result.types_found.append(Type)

        summary = result.summary()
        assert "Types: 5" in summary
        assert "Type0" in summary

    def test_discovery_result_mixed_errors_and_items(self) -> None:
        """Test DiscoveryResult with both items and errors."""
        result = DiscoveryResult(source="test.mixed")

        class User:
            pass

        result.types_found.append(User)
        result.errors.append(ImportError("Some module failed"))

        summary = result.summary()
        assert "Types: 1" in summary
        assert "Errors: 1" in summary

        # count_total should only count found items, not errors
        assert result.count_total() == 1
