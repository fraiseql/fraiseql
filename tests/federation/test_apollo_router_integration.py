"""Tests for Apollo Router/Gateway integration with FraiseQL Federation.

Tests the complete federation workflow including:
- _service query response format
- Entity resolution (_entities query)
- SDL schema correctness
- Directive compatibility
- Reference resolution for gateway composition
"""

import json

import pytest

from fraiseql.federation import (
    clear_entity_registry,
    entity,
    extend_entity,
    external,
    get_default_resolver,
    requires,
    reset_default_resolver,
)
from fraiseql.federation.entities import EntitiesResolver


def setup_function() -> None:
    """Clear registry and reset resolver before each test."""
    clear_entity_registry()
    reset_default_resolver()


class TestGatewayServiceQuery:
    """Tests for _service query response format for Apollo Gateway."""

    def test_service_query_returns_valid_json(self) -> None:
        """Test that _service query returns valid JSON-serializable response."""

        @entity
        class User:
            id: str
            name: str

        resolver = get_default_resolver()
        service = resolver.resolve_service()

        # Should be JSON serializable
        json_str = json.dumps(service)
        assert len(json_str) > 0

    def test_service_query_response_schema(self) -> None:
        """Test that _service query response has correct schema."""

        @entity
        class Product:
            id: str
            name: str

        resolver = get_default_resolver()
        service = resolver.resolve_service()

        # Response should have 'sdl' field
        assert "sdl" in service
        assert isinstance(service["sdl"], str)

    def test_service_query_sdl_contains_key_directive(self) -> None:
        """Test that SDL contains @key directives."""

        @entity
        class User:
            id: str

        resolver = get_default_resolver()
        service = resolver.resolve_service()

        assert "@key" in service["sdl"]
        assert 'fields: "id"' in service["sdl"]

    def test_service_query_with_multiple_entities(self) -> None:
        """Test _service query with multiple federated entities."""

        @entity
        class User:
            id: str
            email: str

        @entity
        class Post:
            id: str
            title: str
            author_id: str

        resolver = get_default_resolver()
        service = resolver.resolve_service()

        sdl = service["sdl"]
        assert "type User" in sdl
        assert "type Post" in sdl
        assert sdl.count("@key") >= 2


class TestGatewayEntityResolution:
    """Tests for _entities query compatibility with Apollo Gateway."""

    def test_entities_resolver_with_user_entities(self) -> None:
        """Test entity resolution for gateway _entities query."""

        @entity
        class User:
            id: str
            name: str

        resolver = EntitiesResolver()

        # Simulate gateway calling _entities
        representations = [{"__typename": "User", "id": "user-1"}]

        # Verify resolver can parse them
        requests = [resolver._parse_representation(rep) for rep in representations]
        assert len(requests) == 1
        assert requests[0].typename == "User"
        assert requests[0].key_value == "user-1"

    def test_entities_resolver_supports_batch(self) -> None:
        """Test that entities resolver supports batch resolution."""

        @entity
        class Product:
            id: str

        resolver = EntitiesResolver()

        # Multiple entities in one query (batching)
        representations = [
            {"__typename": "Product", "id": "prod-1"},
            {"__typename": "Product", "id": "prod-2"},
            {"__typename": "Product", "id": "prod-3"},
        ]

        requests = [resolver._parse_representation(rep) for rep in representations]
        assert len(requests) == 3

    def test_entities_resolver_handles_composite_keys(self) -> None:
        """Test entity resolution with composite keys."""

        @entity(key=["org_id", "user_id"])
        class OrgUser:
            org_id: str
            user_id: str

        resolver = EntitiesResolver()

        # Composite keys not yet supported in entities resolver
        representations = [{"__typename": "OrgUser", "org_id": "org-1", "user_id": "user-1"}]

        # Should raise NotImplementedError (feature not yet implemented)
        with pytest.raises(NotImplementedError, match="Composite keys not yet supported"):
            resolver._parse_representation(representations[0])


class TestGatewayDirectiveSupport:
    """Tests for directive compatibility with Apollo Gateway."""

    def test_directives_reported_correctly(self) -> None:
        """Test that supported directives are reported for gateway."""
        resolver = get_default_resolver()
        directives = resolver.get_supported_directives()

        # Gateway expects these directives
        assert directives["key"] is True
        assert directives["external"] is True
        assert directives["requires"] is True
        assert directives["provides"] is True

    def test_federation_config_includes_directives(self) -> None:
        """Test that federation config includes directive information."""

        @entity
        class User:
            id: str

        resolver = get_default_resolver()
        config = resolver.get_federation_config()

        assert "directives" in config
        assert config["version"] == 2

    def test_external_directive_in_sdl(self) -> None:
        """Test that @external directives appear in SDL for extensions."""

        @extend_entity(key="id")
        class Product:
            id: str = external()
            name: str = external()

        resolver = get_default_resolver()
        service = resolver.resolve_service()

        assert "@external" in service["sdl"]

    def test_requires_provides_directives_in_sdl(self) -> None:
        """Test that computed field directives are in SDL."""

        @entity
        class Product:
            id: str
            price: float

            @requires("price")
            def discounted(self) -> float:
                return self.price * 0.9

        resolver = get_default_resolver()
        service = resolver.resolve_service()

        assert "@requires" in service["sdl"]


class TestGatewaySchemaComposition:
    """Tests for schema composition compatibility."""

    def test_single_subgraph_schema(self) -> None:
        """Test SDL for single subgraph (users service)."""

        @entity
        class User:
            id: str
            name: str
            email: str

        resolver = get_default_resolver()
        sdl = resolver.get_sdl()

        # Should be valid SDL
        assert "type User" in sdl
        assert "@key" in sdl
        assert "id: String!" in sdl
        assert "name: String!" in sdl
        assert "email: String!" in sdl

    def test_extended_schema_composition(self) -> None:
        """Test SDL for schema extension (reviews service extending products)."""

        @extend_entity(key="id")
        class Product:
            id: str = external()
            name: str = external()
            reviews: list

        resolver = get_default_resolver()
        sdl = resolver.get_sdl()

        # Should have extend type keyword
        assert "extend type Product" in sdl
        assert "@external" in sdl

    def test_multi_service_composition_pattern(self) -> None:
        """Test pattern for multi-service composition."""

        # Primary service: users
        @entity
        class User:
            id: str
            username: str

        # Composition: posts service extending User
        @extend_entity(key="id")
        class UserWithPosts:
            id: str = external()
            posts: list

        # Get federation info
        resolver = get_default_resolver()
        sdl = resolver.get_sdl()

        # Should contain both services' types
        assert "type User" in sdl
        assert "extend type" in sdl

    def test_gateway_federation_config_format(self) -> None:
        """Test federation config format matches Apollo Gateway expectations."""

        @entity
        class Product:
            id: str

        resolver = get_default_resolver()
        config = resolver.get_federation_config()

        # Gateway expects these fields
        assert "sdl" in config
        assert "directives" in config
        assert "version" in config
        assert config["version"] == 2


class TestGatewayErrorHandling:
    """Tests for proper error handling in gateway context."""

    def test_unknown_entity_type_error(self) -> None:
        """Test that unknown entity types are properly rejected."""

        @entity
        class User:
            id: str

        resolver = EntitiesResolver()

        # Unknown type should raise error
        with pytest.raises(ValueError, match="Unknown entity type"):
            resolver._parse_representation({"__typename": "UnknownType", "id": "123"})

    def test_missing_key_field_error(self) -> None:
        """Test that missing key fields are properly reported."""

        @entity
        class User:
            id: str

        resolver = EntitiesResolver()

        # Missing key field should raise error
        with pytest.raises(ValueError, match="Missing key field"):
            resolver._parse_representation({"__typename": "User"})

    def test_missing_typename_error(self) -> None:
        """Test that missing __typename is properly reported."""

        @entity
        class User:
            id: str

        resolver = EntitiesResolver()

        # Missing __typename should raise error
        with pytest.raises(ValueError, match="Missing __typename"):
            resolver._parse_representation({"id": "user-1"})


class TestGatewayEdgeCases:
    """Tests for edge cases in gateway integration."""

    def test_empty_schema_handling(self) -> None:
        """Test handling of empty schema (no entities registered)."""
        # Don't register any entities
        resolver = get_default_resolver()
        service = resolver.resolve_service()

        # Empty schema should still be valid response
        assert "sdl" in service
        assert service["sdl"] == ""

    def test_schema_cache_invalidation(self) -> None:
        """Test that schema cache is properly managed."""
        # Start with empty registry (from fixture)
        resolver = get_default_resolver()
        sdl_empty = resolver.get_sdl()
        assert sdl_empty == ""
        assert resolver._cached_sdl == ""

        # Register an entity
        @entity
        class User:
            id: str

        # Cache is still the old empty value
        assert resolver._cached_sdl == ""

        # Clear cache to force regeneration
        resolver.clear_cache()
        assert resolver._cached_sdl is None

        # Now get_sdl() should regenerate with User
        sdl_updated = resolver.get_sdl()
        assert "type User" in sdl_updated
        assert resolver._cached_sdl is not None

    def test_composite_key_gateway_handling(self) -> None:
        """Test that composite keys work correctly with gateway."""

        @entity(key=["tenant_id", "user_id"])
        class TenantUser:
            tenant_id: str
            user_id: str
            role: str

        # Gateway should be able to reference this entity
        resolver = get_default_resolver()
        sdl = resolver.get_sdl()

        assert "TenantUser" in sdl
        assert '@key(fields: "tenant_id user_id")' in sdl

    def test_computed_field_resolution_in_gateway(self) -> None:
        """Test computed fields work correctly in gateway context."""

        @entity
        class Order:
            id: str
            subtotal: float
            tax_rate: float

            @requires("subtotal tax_rate")
            def total_with_tax(self) -> float:
                return self.subtotal + (self.subtotal * self.tax_rate)

        resolver = get_default_resolver()
        sdl = resolver.get_sdl()

        # Computed field should appear in SDL
        assert "total_with_tax" in sdl
        assert "@requires" in sdl


class TestGatewayIntegrationFlow:
    """Tests for complete gateway integration flow."""

    def test_complete_federation_gateway_flow(self) -> None:
        """Test complete flow: _service query -> SDL -> gateway composition."""

        # Define entities
        @entity
        class User:
            id: str
            name: str
            email: str

        @entity
        class Post:
            id: str
            title: str
            author_id: str

        # Step 1: Gateway calls _service query
        resolver = get_default_resolver()
        service_response = resolver.resolve_service()

        # Step 2: Gateway receives SDL
        sdl = service_response["sdl"]
        assert "@key" in sdl
        assert "type User" in sdl
        assert "type Post" in sdl

        # Step 3: Gateway parses directives
        directives = resolver.get_supported_directives()
        assert directives["key"] is True

        # Step 4: Gateway uses entities resolver for _entities query
        entities_resolver = EntitiesResolver()
        types = entities_resolver.get_supported_types()
        assert "User" in types
        assert "Post" in types

    def test_multi_subgraph_federation_flow(self) -> None:
        """Test federation flow with multiple subgraphs."""

        # Primary service: users
        @entity
        class User:
            id: str
            username: str

        # Composition: posts service extending users
        @extend_entity(key="id")
        class UserWithPosts:
            id: str = external()
            posts: list

        # Get federation info
        resolver = get_default_resolver()
        config = resolver.get_federation_config()

        # Should be valid for gateway
        assert config["version"] == 2
        assert len(config["sdl"]) > 0
        assert "extend type" in config["sdl"]

    def test_gateway_cross_service_reference(self) -> None:
        """Test cross-service entity references."""

        @entity
        class User:
            id: str
            name: str

        @entity
        class Post:
            id: str
            title: str
            author_id: str  # References User.id from other service

        # Both entities should be queryable
        resolver = get_default_resolver()
        sdl = resolver.get_sdl()

        assert "type User" in sdl
        assert "type Post" in sdl
        assert "author_id: String!" in sdl


class TestGatewayPerformance:
    """Tests for performance characteristics in gateway integration."""

    def test_sdl_caching_performance(self) -> None:
        """Test that SDL caching improves performance."""

        @entity
        class Product:
            id: str
            name: str

        resolver = get_default_resolver()

        # First call generates SDL
        resolver.clear_cache()
        sdl1 = resolver.get_sdl()

        # Second call should use cache
        sdl2 = resolver.get_sdl()

        # Results should be identical
        assert sdl1 == sdl2

    def test_resolver_singleton_efficiency(self) -> None:
        """Test that singleton resolver is efficiently reused."""
        resolver1 = get_default_resolver()
        resolver2 = get_default_resolver()

        # Should be same instance
        assert resolver1 is resolver2


if __name__ == "__main__":
    pytest.main([__file__, "-v"])
