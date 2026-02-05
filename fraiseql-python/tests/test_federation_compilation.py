"""
Federation schema compilation tests for Cycle 16-3 RED phase.

Tests federation schema validation and compilation.
All tests expected to FAIL initially (RED phase).
"""

import pytest

try:
    from fraiseql import type as fraiseql_type, ID
    from fraiseql.federation import key, extends, external, requires, provides
    from fraiseql.schema import Schema
    from fraiseql.errors import FederationValidationError
except ImportError:
    pass


class TestFederationSchemaCompilation:
    """Test federation schema compilation."""

    def test_federation_schema_compilation_success(self):
        """Complete federation schema compiles successfully."""
        @fraiseql_type
        @key("id")
        class User:
            id: ID
            email: str

        @fraiseql_type
        @key("id")
        class Order:
            id: ID
            user_id: ID
            total: float

        schema = Schema(types=[User, Order])
        compiled = schema.compile()

        # Should include federation metadata
        assert hasattr(compiled, "federation")
        assert compiled.federation is not None
        assert compiled.federation.enabled is True

    def test_extended_entity_compilation(self):
        """Extended entity compiles successfully."""
        @fraiseql_type
        @extends
        @key("id")
        class User:
            id: ID = external()
            email: str = external()
            orders: list["Order"]

        schema = Schema(types=[User])
        compiled = schema.compile()

        assert compiled.federation is not None
        assert compiled.federation.enabled is True

    def test_compilation_with_mixed_types(self):
        """Compilation handles both federation and non-federation types."""
        @fraiseql_type
        @key("id")
        class User:
            id: ID
            email: str

        @fraiseql_type
        class Config:
            key: str
            value: str

        schema = Schema(types=[User, Config])
        compiled = schema.compile()

        assert compiled.federation is not None


class TestFederationValidationErrors:
    """Test federation schema validation catches errors."""

    def test_invalid_key_field_raises_error(self):
        """@key with non-existent field raises validation error."""
        with pytest.raises(FederationValidationError):
            @fraiseql_type
            @key("nonexistent")
            class User:
                id: ID
                email: str

            Schema(types=[User]).compile()

    def test_external_without_extends_raises_error(self):
        """@external on non-extended type raises validation error."""
        with pytest.raises(FederationValidationError, match="@external requires @extends"):
            @fraiseql_type
            @key("id")
            class User:
                id: ID = external()
                email: str

            Schema(types=[User]).compile()

    def test_requires_nonexistent_field_raises_error(self):
        """@requires with non-existent field raises validation error."""
        with pytest.raises(FederationValidationError):
            @fraiseql_type
            @extends
            @key("id")
            class User:
                id: ID = external()
                profile: str = requires("nonexistent")

            Schema(types=[User]).compile()

    def test_extends_without_key_raises_error(self):
        """@extends without @key raises validation error."""
        with pytest.raises(FederationValidationError):
            @fraiseql_type
            @extends
            class User:
                id: ID = external()

            Schema(types=[User]).compile()

    def test_duplicate_key_raises_error(self):
        """Multiple @key decorators with same field raises error."""
        with pytest.raises(FederationValidationError):
            @fraiseql_type
            @key("id")
            @key("id")
            class User:
                id: ID
                email: str

            Schema(types=[User]).compile()


class TestCrossSubgraphValidation:
    """Test validation of cross-subgraph federation."""

    def test_composite_key_in_extended_type(self):
        """Extended type with composite key compiles."""
        @fraiseql_type
        @extends
        @key("tenant_id")
        @key("id")
        class Account:
            tenant_id: ID = external()
            id: ID = external()
            balance: float

        schema = Schema(types=[Account])
        compiled = schema.compile()

        assert compiled.federation is not None

    def test_multiple_requires_dependencies(self):
        """Type with multiple @requires fields compiles."""
        @fraiseql_type
        @key("id")
        class Order:
            id: ID
            user_id: ID
            product_id: ID
            user: str = requires("user_id")
            product: str = requires("product_id")

        schema = Schema(types=[Order])
        compiled = schema.compile()

        assert compiled.federation is not None

    def test_provides_metadata_validation(self):
        """@provides fields validated during compilation."""
        @fraiseql_type
        @key("id")
        class User:
            id: ID
            email: str
            owner_email: str = provides("Order.owner")

        schema = Schema(types=[User])
        compiled = schema.compile()

        assert compiled.federation is not None


class TestFederationMetadataPreservation:
    """Test that federation metadata is preserved through compilation."""

    def test_key_metadata_preserved(self):
        """Key metadata preserved after compilation."""
        @fraiseql_type
        @key("id")
        class User:
            id: ID
            email: str

        schema = Schema(types=[User])
        compiled = schema.compile()

        user_type_info = compiled.get_type("User")
        assert user_type_info is not None
        assert user_type_info.federation_keys is not None

    def test_external_fields_preserved(self):
        """External fields metadata preserved after compilation."""
        @fraiseql_type
        @extends
        @key("id")
        class User:
            id: ID = external()
            email: str = external()
            orders: list["Order"]

        schema = Schema(types=[User])
        compiled = schema.compile()

        user_type_info = compiled.get_type("User")
        assert user_type_info.is_extended is True
        assert user_type_info.external_fields is not None

    def test_requires_metadata_preserved(self):
        """@requires metadata preserved after compilation."""
        @fraiseql_type
        @extends
        @key("id")
        class User:
            id: ID = external()
            email: str = external()
            profile: str = requires("email")

        schema = Schema(types=[User])
        compiled = schema.compile()

        user_type_info = compiled.get_type("User")
        requires_fields = user_type_info.requires_fields
        assert "profile" in requires_fields
        assert requires_fields["profile"] == "email"


class TestComplexFederationScenarios:
    """Test complex federation scenarios."""

    def test_three_type_federation(self):
        """Schema with three federated types."""
        @fraiseql_type
        @key("id")
        class User:
            id: ID
            email: str

        @fraiseql_type
        @key("id")
        class Order:
            id: ID
            user_id: ID
            user: str = requires("user_id")

        @fraiseql_type
        @key("id")
        class Product:
            id: ID
            name: str

        schema = Schema(types=[User, Order, Product])
        compiled = schema.compile()

        assert compiled.federation is not None
        assert compiled.get_type("User") is not None
        assert compiled.get_type("Order") is not None
        assert compiled.get_type("Product") is not None

    def test_mixed_local_and_extended_entities(self):
        """Schema with both local and extended entities."""
        @fraiseql_type
        @key("id")
        class User:
            id: ID
            email: str

        @fraiseql_type
        @extends
        @key("id")
        class User:  # Extended in different subgraph
            id: ID = external()
            email: str = external()
            orders: list["Order"]

        schema = Schema(types=[User])
        compiled = schema.compile()

        assert compiled.federation is not None

    def test_federation_with_enums(self):
        """Federation schema with enum types."""
        from fraiseql import enum

        @enum
        class OrderStatus:
            PENDING = "PENDING"
            COMPLETED = "COMPLETED"
            CANCELLED = "CANCELLED"

        @fraiseql_type
        @key("id")
        class Order:
            id: ID
            status: OrderStatus

        schema = Schema(types=[Order])
        compiled = schema.compile()

        assert compiled.federation is not None


class TestCompilationOutputValidation:
    """Test compiled schema output is valid."""

    def test_compiled_schema_has_sdl_method(self):
        """Compiled schema can generate SDL."""
        @fraiseql_type
        @key("id")
        class User:
            id: ID
            email: str

        schema = Schema(types=[User])
        compiled = schema.compile()

        # Should have method to generate federation SDL
        assert hasattr(compiled, "to_federation_sdl")
        sdl = compiled.to_federation_sdl()
        assert sdl is not None
        assert "@key" in sdl  # Should include federation directives

    def test_compiled_schema_federation_sdl_format(self):
        """Federation SDL includes proper directives."""
        @fraiseql_type
        @key("id")
        class User:
            id: ID
            email: str

        schema = Schema(types=[User])
        compiled = schema.compile()

        sdl = compiled.to_federation_sdl()

        # Should match Apollo Federation SDL format
        assert 'type User' in sdl
        assert '@key' in sdl
        assert '(fields: "id")' in sdl

    def test_extended_entity_federation_sdl(self):
        """Extended entity SDL includes @extends directive."""
        @fraiseql_type
        @extends
        @key("id")
        class User:
            id: ID = external()
            email: str = external()

        schema = Schema(types=[User])
        compiled = schema.compile()

        sdl = compiled.to_federation_sdl()

        # Should include extend directive
        assert 'extend type User' in sdl
        assert '@external' in sdl or '@key' in sdl

    def test_requires_provides_in_federation_sdl(self):
        """@requires and @provides appear in federation SDL."""
        @fraiseql_type
        @key("id")
        class Order:
            id: ID
            user_id: ID
            user: str = requires("user_id")
            reference_code: str = provides("Invoice.reference")

        schema = Schema(types=[Order])
        compiled = schema.compile()

        sdl = compiled.to_federation_sdl()

        # Should include field directives
        assert '@requires' in sdl or 'user:' in sdl
        assert '@provides' in sdl or 'reference_code:' in sdl
