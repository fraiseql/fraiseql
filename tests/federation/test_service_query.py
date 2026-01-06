"""Tests for Apollo Federation _service query implementation."""

import pytest

from fraiseql.federation import entity, extend_entity, external
from fraiseql.federation.service_query import (
    ServiceQueryResolver,
    create_service_resolver,
    get_default_resolver,
    reset_default_resolver,
)


class TestServiceQueryResolver:
    """Tests for ServiceQueryResolver."""

    def test_resolver_initialization(self) -> None:
        """Test creating a service query resolver."""
        resolver = ServiceQueryResolver()
        assert resolver.cache_sdl is True

    def test_resolver_with_caching_disabled(self) -> None:
        """Test resolver with caching disabled."""
        resolver = ServiceQueryResolver(cache_sdl=False)
        assert resolver.cache_sdl is False

    def test_resolve_service_returns_sdl(self) -> None:
        """Test that _service query returns SDL."""
        from fraiseql.federation import clear_entity_registry

        clear_entity_registry()

        @entity
        class User:
            id: str
            name: str

        resolver = ServiceQueryResolver()
        result = resolver.resolve_service()

        assert "sdl" in result
        assert isinstance(result["sdl"], str)
        assert "@key" in result["sdl"]

    def test_get_sdl_basic(self) -> None:
        """Test getting SDL from resolver."""
        from fraiseql.federation import clear_entity_registry

        clear_entity_registry()

        @entity
        class User:
            id: str
            name: str

        resolver = ServiceQueryResolver()
        sdl = resolver.get_sdl()

        assert "type User" in sdl
        assert "@key" in sdl

    def test_sdl_caching(self) -> None:
        """Test that SDL is cached when enabled."""
        from fraiseql.federation import clear_entity_registry

        clear_entity_registry()

        @entity
        class User:
            id: str

        resolver = ServiceQueryResolver(cache_sdl=True)

        # First call generates SDL
        sdl1 = resolver.get_sdl()
        # Second call should return cached version
        sdl2 = resolver.get_sdl()

        assert sdl1 == sdl2
        assert resolver._cached_sdl is not None

    def test_sdl_no_caching(self) -> None:
        """Test that SDL is not cached when disabled."""
        from fraiseql.federation import clear_entity_registry

        clear_entity_registry()

        @entity
        class User:
            id: str

        resolver = ServiceQueryResolver(cache_sdl=False)

        # Multiple calls should not cache
        sdl1 = resolver.get_sdl()
        assert resolver._cached_sdl is None

        sdl2 = resolver.get_sdl()
        assert sdl1 == sdl2
        assert resolver._cached_sdl is None

    def test_clear_cache(self) -> None:
        """Test clearing SDL cache."""
        from fraiseql.federation import clear_entity_registry

        clear_entity_registry()

        @entity
        class User:
            id: str

        resolver = ServiceQueryResolver(cache_sdl=True)
        resolver.get_sdl()
        assert resolver._cached_sdl is not None

        resolver.clear_cache()
        assert resolver._cached_sdl is None


class TestSupportedDirectives:
    """Tests for supported directives."""

    def test_get_supported_directives(self) -> None:
        """Test getting supported directives."""
        resolver = ServiceQueryResolver()
        directives = resolver.get_supported_directives()

        assert isinstance(directives, dict)
        assert "key" in directives
        assert "external" in directives
        assert "requires" in directives
        assert "provides" in directives

    def test_supported_directives_include_federation_lite(self) -> None:
        """Test that Federation Lite directives are supported."""
        resolver = ServiceQueryResolver()
        directives = resolver.get_supported_directives()

        # Federation Lite
        assert directives["key"] is True

    def test_supported_directives_include_federation_standard(self) -> None:
        """Test that Federation Standard directives are supported."""
        resolver = ServiceQueryResolver()
        directives = resolver.get_supported_directives()

        # Federation Standard
        assert directives["external"] is True
        assert directives["requires"] is True
        assert directives["provides"] is True

    def test_is_directive_supported(self) -> None:
        """Test checking if directive is supported."""
        resolver = ServiceQueryResolver()

        assert resolver.is_directive_supported("key") is True
        assert resolver.is_directive_supported("external") is True
        assert resolver.is_directive_supported("requires") is True
        assert resolver.is_directive_supported("provides") is True

    def test_unsupported_directives(self) -> None:
        """Test unsupported directives."""
        resolver = ServiceQueryResolver()

        # @reference not implemented
        assert resolver.is_directive_supported("reference") is False
        # Non-existent directive
        assert resolver.is_directive_supported("nonexistent") is False

    def test_supported_directives_returns_copy(self) -> None:
        """Test that modifications don't affect internal state."""
        resolver = ServiceQueryResolver()
        directives1 = resolver.get_supported_directives()
        directives1["key"] = False

        directives2 = resolver.get_supported_directives()
        assert directives2["key"] is True  # Should still be True


class TestFederationConfig:
    """Tests for federation configuration."""

    def test_get_federation_config(self) -> None:
        """Test getting federation configuration."""
        from fraiseql.federation import clear_entity_registry

        clear_entity_registry()

        @entity
        class User:
            id: str

        resolver = ServiceQueryResolver()
        config = resolver.get_federation_config()

        assert "sdl" in config
        assert "directives" in config
        assert "version" in config

    def test_federation_config_version(self) -> None:
        """Test federation version in config."""
        resolver = ServiceQueryResolver()
        config = resolver.get_federation_config()

        assert config["version"] == 2  # Apollo Federation 2.0

    def test_federation_config_contains_sdl(self) -> None:
        """Test that config contains complete SDL."""
        from fraiseql.federation import clear_entity_registry

        clear_entity_registry()

        @entity
        class User:
            id: str
            name: str

        resolver = ServiceQueryResolver()
        config = resolver.get_federation_config()

        assert "type User" in config["sdl"]
        assert "@key" in config["sdl"]


class TestCreateServiceResolver:
    """Tests for create_service_resolver factory function."""

    def test_create_with_caching(self) -> None:
        """Test creating resolver with caching."""
        resolver = create_service_resolver(cache_sdl=True)
        assert isinstance(resolver, ServiceQueryResolver)
        assert resolver.cache_sdl is True

    def test_create_without_caching(self) -> None:
        """Test creating resolver without caching."""
        resolver = create_service_resolver(cache_sdl=False)
        assert isinstance(resolver, ServiceQueryResolver)
        assert resolver.cache_sdl is False


class TestDefaultResolver:
    """Tests for default singleton resolver."""

    def test_get_default_resolver(self) -> None:
        """Test getting default resolver."""
        resolver = get_default_resolver()
        assert isinstance(resolver, ServiceQueryResolver)

    def test_default_resolver_singleton(self) -> None:
        """Test that default resolver is a singleton."""
        resolver1 = get_default_resolver()
        resolver2 = get_default_resolver()
        assert resolver1 is resolver2

    def test_reset_default_resolver(self) -> None:
        """Test resetting default resolver."""
        from fraiseql.federation import clear_entity_registry

        clear_entity_registry()

        @entity
        class User:
            id: str

        resolver1 = get_default_resolver()
        resolver1.get_sdl()

        reset_default_resolver()
        resolver2 = get_default_resolver()

        assert resolver2 is resolver1  # Same instance
        assert resolver2._cached_sdl is None  # Cache cleared


class TestServiceQueryIntegration:
    """Integration tests for service query."""

    def test_service_query_with_single_entity(self) -> None:
        """Test _service query with single entity."""
        from fraiseql.federation import clear_entity_registry

        clear_entity_registry()

        @entity
        class Product:
            id: str
            name: str
            price: float

        resolver = ServiceQueryResolver()
        result = resolver.resolve_service()

        sdl = result["sdl"]
        assert 'type Product @key(fields: "id")' in sdl
        assert "id: String!" in sdl
        assert "name: String!" in sdl
        assert "price: Float!" in sdl

    def test_service_query_with_multiple_entities(self) -> None:
        """Test _service query with multiple entities."""
        from fraiseql.federation import clear_entity_registry

        clear_entity_registry()

        @entity
        class User:
            id: str
            name: str

        @entity
        class Post:
            id: str
            title: str

        resolver = ServiceQueryResolver()
        result = resolver.resolve_service()

        sdl = result["sdl"]
        assert "type User" in sdl
        assert "type Post" in sdl

    def test_service_query_with_extensions(self) -> None:
        """Test _service query with extended entities."""
        from fraiseql.federation import clear_entity_registry

        clear_entity_registry()

        @entity
        class User:
            id: str
            name: str

        @extend_entity(key="id")
        class Product:
            id: str = external()
            reviews: list

        resolver = ServiceQueryResolver()
        result = resolver.resolve_service()

        sdl = result["sdl"]
        assert "extend type Product" in sdl
        assert "@external" in sdl

    def test_service_query_with_computed_fields(self) -> None:
        """Test _service query includes computed fields."""
        from fraiseql.federation import clear_entity_registry, requires

        clear_entity_registry()

        @entity
        class Product:
            id: str
            price: float

            @requires("price")
            def discounted(self) -> float:
                return self.price * 0.9

        resolver = ServiceQueryResolver()
        result = resolver.resolve_service()

        sdl = result["sdl"]
        assert "discounted: JSON" in sdl
        assert '@requires(fields: "price")' in sdl

    def test_full_federation_workflow(self) -> None:
        """Test complete federation workflow."""
        from fraiseql.federation import clear_entity_registry

        clear_entity_registry()

        @entity
        class User:
            id: str
            name: str

        # Create resolver
        resolver = ServiceQueryResolver()

        # Resolve _service query
        service = resolver.resolve_service()
        assert "sdl" in service

        # Get directives
        directives = resolver.get_supported_directives()
        assert directives["key"] is True
        assert directives["external"] is True

        # Get config
        config = resolver.get_federation_config()
        assert config["version"] == 2
        assert len(config["sdl"]) > 0


class TestServiceQueryEdgeCases:
    """Tests for edge cases in service query."""

    def test_service_query_with_empty_registry(self) -> None:
        """Test _service query when no entities registered."""
        from fraiseql.federation import clear_entity_registry

        clear_entity_registry()
        resolver = ServiceQueryResolver()
        result = resolver.resolve_service()

        assert result["sdl"] == ""

    def test_service_query_caching_with_clear(self) -> None:
        """Test cache behavior with explicit clear."""
        from fraiseql.federation import clear_entity_registry

        clear_entity_registry()

        @entity
        class User:
            id: str

        resolver = ServiceQueryResolver(cache_sdl=True)
        sdl1 = resolver.get_sdl()

        resolver.clear_cache()
        sdl2 = resolver.get_sdl()

        assert sdl1 == sdl2
        assert resolver._cached_sdl is not None  # Cache repopulated

    def test_supported_directives_immutability(self) -> None:
        """Test that supported directives can't be modified internally."""
        resolver = ServiceQueryResolver()
        directives = resolver.get_supported_directives()

        # Modify returned dict
        directives["fake"] = True

        # Should not affect resolver
        directives2 = resolver.get_supported_directives()
        assert "fake" not in directives2


if __name__ == "__main__":
    pytest.main([__file__, "-v"])
