"""Tests for SDL generation from Federation metadata."""

import pytest

from fraiseql.federation import entity, extend_entity, external, provides, requires
from fraiseql.federation.sdl_generator import (
    SDLGenerator,
    generate_entity_sdl,
    generate_schema_sdl,
)


class TestSDLGeneratorBasics:
    """Tests for basic SDL generation."""

    def test_simple_entity_sdl(self):
        """Test SDL generation for simple entity."""
        @entity
        class User:
            id: str
            name: str

        sdl = generate_entity_sdl(User)

        assert 'type User @key(fields: "id")' in sdl
        assert "id: String!" in sdl
        assert "name: String!" in sdl
        assert sdl.count("}") == 1

    def test_entity_with_various_types(self):
        """Test SDL generation with different field types."""
        @entity
        class Product:
            id: str
            name: str
            price: float
            in_stock: bool
            quantity: int

        sdl = generate_entity_sdl(Product)

        assert "id: String!" in sdl
        assert "name: String!" in sdl
        assert "price: Float!" in sdl
        assert "in_stock: Boolean!" in sdl
        assert "quantity: Int!" in sdl

    def test_entity_with_optional_fields(self):
        """Test SDL generation with optional fields."""
        from typing import Optional

        @entity
        class Article:
            id: str
            title: str
            description: Optional[str]

        sdl = generate_entity_sdl(Article)

        assert "title: String!" in sdl
        assert "description: String" in sdl
        assert "description: String!" not in sdl

    def test_extended_entity_sdl(self):
        """Test SDL generation for extended entity."""
        @extend_entity(key="id")
        class Product:
            id: str = external()
            name: str = external()
            reviews: list

        sdl = generate_entity_sdl(Product)

        assert 'extend type Product @key(fields: "id")' in sdl
        assert "id: String! @external" in sdl
        assert "name: String! @external" in sdl

    def test_composite_key_sdl(self):
        """Test SDL generation with composite key."""
        @entity(key=["org_id", "user_id"])
        class OrgUser:
            org_id: str
            user_id: str
            role: str

        sdl = generate_entity_sdl(OrgUser)

        assert '@key(fields: "org_id user_id")' in sdl

    def test_unregistered_entity_raises_error(self):
        """Test that unregistered entity raises error."""
        class NotRegistered:
            id: str

        with pytest.raises(ValueError, match="not a registered entity"):
            generate_entity_sdl(NotRegistered)


class TestSDLGeneratorTypes:
    """Tests for type resolution."""

    def test_type_map_string(self):
        """Test string type mapping."""
        gen = SDLGenerator()
        assert gen._resolve_graphql_type(str) == "String!"

    def test_type_map_int(self):
        """Test int type mapping."""
        gen = SDLGenerator()
        assert gen._resolve_graphql_type(int) == "Int!"

    def test_type_map_float(self):
        """Test float type mapping."""
        gen = SDLGenerator()
        assert gen._resolve_graphql_type(float) == "Float!"

    def test_type_map_bool(self):
        """Test bool type mapping."""
        gen = SDLGenerator()
        assert gen._resolve_graphql_type(bool) == "Boolean!"

    def test_optional_string(self):
        """Test optional string type."""
        from typing import Optional

        gen = SDLGenerator()
        result = gen._resolve_graphql_type(Optional[str])
        assert "String" in result
        assert "!" not in result or "[" in result

    def test_list_type(self):
        """Test list type resolution."""
        gen = SDLGenerator()
        result = gen._resolve_graphql_type(list[str])
        assert "[" in result
        assert "String" in result

    def test_string_type_name(self):
        """Test string type names."""
        gen = SDLGenerator()
        assert "String" in gen._resolve_graphql_type("str")
        assert "Int" in gen._resolve_graphql_type("int")


class TestSDLGeneratorDirectives:
    """Tests for directive generation."""

    def test_external_directive(self):
        """Test @external directive generation."""
        @extend_entity(key="id")
        class Product:
            id: str = external()
            name: str = external()
            price: str = external()

        sdl = generate_entity_sdl(Product)

        assert "id: String! @external" in sdl
        assert "name: String! @external" in sdl
        assert "price: String! @external" in sdl

    def test_key_directive_single(self):
        """Test @key directive with single field."""
        @entity(key="id")
        class User:
            id: str
            name: str

        sdl = generate_entity_sdl(User)
        assert '@key(fields: "id")' in sdl

    def test_key_directive_composite(self):
        """Test @key directive with composite key."""
        @entity(key=["tenant_id", "user_id"])
        class TenantUser:
            tenant_id: str
            user_id: str
            role: str

        sdl = generate_entity_sdl(TenantUser)
        assert '@key(fields: "tenant_id user_id")' in sdl


class TestSDLGeneratorComputedFields:
    """Tests for computed field SDL generation."""

    def test_computed_field_with_requires(self):
        """Test SDL generation for field with @requires."""
        @entity
        class Product:
            id: str
            price: float

            @requires("price")
            def discounted(self) -> float:
                return self.price * 0.9

        sdl = generate_entity_sdl(Product)

        assert "id: String!" in sdl
        assert "price: Float!" in sdl
        assert 'discounted: JSON @requires(fields: "price")' in sdl

    def test_computed_field_with_provides(self):
        """Test SDL generation for field with @provides."""
        @entity
        class User:
            id: str
            name: str

            @provides("id name")
            async def posts(self):
                pass

        sdl = generate_entity_sdl(User)

        assert 'posts: JSON @provides(fields: "id name")' in sdl

    def test_computed_field_with_both_directives(self):
        """Test computed field with both @requires and @provides."""
        @entity
        class Article:
            id: str
            title: str
            content: str

            @requires("content")
            @provides("id")
            async def summary(self):
                pass

        sdl = generate_entity_sdl(Article)

        assert "summary: JSON" in sdl
        assert '@requires(fields: "content")' in sdl
        assert '@provides(fields: "id")' in sdl


class TestSDLGeneratorSchema:
    """Tests for complete schema generation."""

    def test_empty_registry(self):
        """Test SDL generation with no registered entities."""
        from fraiseql.federation import clear_entity_registry

        clear_entity_registry()
        sdl = generate_schema_sdl()
        assert sdl == ""

    def test_single_entity_schema(self):
        """Test schema generation with single entity."""
        from fraiseql.federation import clear_entity_registry

        clear_entity_registry()

        @entity
        class User:
            id: str
            name: str

        sdl = generate_schema_sdl()

        assert 'type User @key(fields: "id")' in sdl
        assert "id: String!" in sdl
        assert "name: String!" in sdl

    def test_multiple_entities_schema(self):
        """Test schema generation with multiple entities."""
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
            author_id: str

        sdl = generate_schema_sdl()

        # Both entities should be present
        assert 'type User @key(fields: "id")' in sdl
        assert 'type Post @key(fields: "id")' in sdl

        # Entities should be separated by blank lines
        assert "\n\n" in sdl

    def test_mixed_entities_and_extensions(self):
        """Test schema with both entities and extensions."""
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

        sdl = generate_schema_sdl()

        assert 'type User @key(fields: "id")' in sdl
        assert 'extend type Product @key(fields: "id")' in sdl


class TestSDLGeneratorFormatting:
    """Tests for SDL formatting."""

    def test_proper_indentation(self):
        """Test that fields are properly indented."""
        @entity
        class User:
            id: str
            name: str

        sdl = generate_entity_sdl(User)

        lines = sdl.split("\n")
        # Check that fields are indented
        field_lines = [line for line in lines if ":" in line and "@key" not in line]
        for line in field_lines:
            assert line.startswith("  ")

    def test_closing_brace(self):
        """Test that SDL has closing brace."""
        @entity
        class User:
            id: str

        sdl = generate_entity_sdl(User)

        assert sdl.endswith("}")

    def test_no_leading_closing_brace(self):
        """Test that SDL doesn't have extra closing braces."""
        @entity
        class User:
            id: str
            name: str

        sdl = generate_entity_sdl(User)

        # Should have exactly one closing brace
        assert sdl.count("}") == 1


class TestSDLGeneratorRealWorld:
    """Real-world SDL generation scenarios."""

    def test_ecommerce_product_extension(self):
        """Test SDL for product extension with reviews."""
        from fraiseql.federation import clear_entity_registry

        clear_entity_registry()

        @extend_entity(key="id")
        class Product:
            id: str = external()
            name: str = external()
            price: float = external()

            reviews: list

            @requires("price")
            def discounted_price(self) -> float:
                return self.price * 0.9

        sdl = generate_entity_sdl(Product)

        # Check key fields
        assert '@key(fields: "id")' in sdl

        # Check external fields
        assert "id: String! @external" in sdl
        assert "name: String! @external" in sdl
        assert "price: Float! @external" in sdl

        # Check new fields
        assert "reviews:" in sdl and "[" in sdl

        # Check computed fields
        assert "discounted_price: JSON" in sdl

    def test_user_posts_extension(self):
        """Test SDL for user extension with posts."""
        from fraiseql.federation import clear_entity_registry

        clear_entity_registry()

        @extend_entity(key="id")
        class User:
            id: str = external()
            username: str = external()
            email: str = external()

            posts: list

            @provides("id username")
            async def user_posts(self):
                pass

        sdl = generate_entity_sdl(User)

        assert 'extend type User @key(fields: "id")' in sdl
        assert "id: String! @external" in sdl
        assert "username: String! @external" in sdl
        assert "email: String! @external" in sdl
        assert "posts:" in sdl and "[" in sdl
        assert 'user_posts: JSON @provides(fields: "id username")' in sdl

    def test_complex_entity_with_all_features(self):
        """Test complex entity with all SDL features."""
        from fraiseql.federation import clear_entity_registry

        clear_entity_registry()

        @entity(key=["org_id", "user_id"])
        class OrgUser:
            org_id: str
            user_id: str
            role: str
            permissions: list

            @requires("role")
            def permission_level(self) -> int:
                return 1

        sdl = generate_entity_sdl(OrgUser)

        # Check composite key
        assert '@key(fields: "org_id user_id")' in sdl

        # Check regular fields
        assert "role: String!" in sdl
        assert "permissions:" in sdl and "[" in sdl

        # Check computed fields
        assert "permission_level: JSON" in sdl


class TestSDLGeneratorEdgeCases:
    """Tests for edge cases."""

    def test_entity_with_no_fields(self):
        """Test entity with no additional fields beyond key."""
        @entity
        class Marker:
            id: str

        sdl = generate_entity_sdl(Marker)

        assert 'type Marker @key(fields: "id")' in sdl
        assert "id: String!" in sdl

    def test_entity_with_special_field_names(self):
        """Test entity with underscore-prefixed fields."""
        @entity
        class User:
            id: str
            name: str
            _internal: str

        sdl = generate_entity_sdl(User)

        # Should not include _internal field
        assert "_internal" not in sdl
        assert "name: String!" in sdl

    def test_entity_extension_without_new_fields(self):
        """Test extension that only marks existing fields as external."""
        @extend_entity(key="id")
        class Product:
            id: str = external()
            name: str = external()

        sdl = generate_entity_sdl(Product)

        assert "extend type Product" in sdl
        assert "@external" in sdl


if __name__ == "__main__":
    pytest.main([__file__, "-v"])
